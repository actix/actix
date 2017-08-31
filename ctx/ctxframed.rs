#![allow(dead_code)]

use std;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;

use futures::{Async, AsyncSink, Future, Poll, Stream, Sink};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::{Framed, Encoder, Decoder};
use tokio_core::reactor::Handle;

use fut::CtxFuture;
use ctx::{Service, CtxMessage};

bitflags!(
    /// State Bitflags
    struct State: u16 {
        /// Service is started
        const STARTED = 0b00000001;
    }
);


pub enum CtxFramedResult<T: FramedContextAware> {
    Sent,
    Ok(<<T as FramedContextAware>::Codec as Decoder>::Item),
    Err(<<T as FramedContextAware>::Codec as Decoder>::Error),
    SinkErr(<T::Codec as Encoder>::Error)
}

enum Item<T: FramedContextAware> {
    CtxFuture(Box<CtxServiceCtxFuture<T>>),
    CtxSpawnFuture(Box<CtxSpawnFuture<T>>),
    Future(Box<CtxFramedServiceFuture<T>>),
    Stream(Box<CtxFramedServiceStream<T>>),
    FutStream(Box<CtxFramedServiceFutStream<T>>),
}

type CtxServiceCtxFuture<T> =
    CtxFuture<Item=<<T as FramedContextAware>::Message as CtxMessage>::Item,
              Error=<<T as FramedContextAware>::Message as CtxMessage>::Error,
              Context=<T as FramedContextAware>::Context, Service=CtxFramedService<T>>;

type CtxSpawnFuture<T> =
    CtxFuture<Item=(), Error=(),
              Context=<T as FramedContextAware>::Context, Service=CtxFramedService<T>>;

pub type CtxFramedServiceFuture<T> =
    Future<Item=<<T as FramedContextAware>::Codec as Decoder>::Item,
           Error=<<T as FramedContextAware>::Codec as Decoder>::Error>;

pub type CtxFramedServiceStream<T> =
    Stream<Item=<<T as FramedContextAware>::Codec as Decoder>::Item,
           Error=<<T as FramedContextAware>::Codec as Decoder>::Error>;

pub type CtxFramedServiceFutStream<T> =
    Future<Item=Box<CtxFramedServiceStream<T>>,
           Error=<<T as FramedContextAware>::Codec as Decoder>::Error>;

pub trait FramedContextAware: Sized + 'static
{
    type Context;
    type Message: CtxMessage<Error=std::io::Error>;
    type Result: CtxMessage;
    type FramedContext: AsyncRead + AsyncWrite;
    type Codec: Encoder + Decoder<Item=<Self::Message as CtxMessage>::Item,
                                  Error=<Self::Message as CtxMessage>::Error>;

    /// Run service for `T` and stream `S`
    fn run(self, ctx: Self::Context,
           framed: Framed<Self::FramedContext, Self::Codec>, handle: &Handle)
    {
        CtxFramedService {
            flags: State::empty(),
            ctx: Rc::new(RefCell::new(ctx)),
            exec: self,
            handle: handle.clone(),
            framed: framed,
            items: Vec::new(),
            sink_items: VecDeque::new(),
            sink_flushed: true,
        }.run()
    }

    /// Spawn service for `T` and stream `S`
    fn spawn(self, srv: &Service<Self::Context>, framed: Framed<Self::FramedContext, Self::Codec>)
    {
        CtxFramedService {
            flags: State::empty(),
            ctx: srv.clone(),
            exec: self,
            handle: srv.handle().clone(),
            framed: framed,
            items: Vec::new(),
            sink_items: VecDeque::new(),
            sink_flushed: true,
        }.run()
    }

    /// Build service for `T` and stream `S`
    #[must_use = "service do nothing unless polled"]
    fn build(self, ctx: Self::Context,
             framed: Framed<Self::FramedContext, Self::Codec>, handle: &Handle)
             -> CtxFramedServiceBuilder<Self>
    {
        CtxFramedServiceBuilder {
            srv: CtxFramedService {
                flags: State::empty(),
                ctx: Rc::new(RefCell::new(ctx)),
                exec: self,
                handle: handle.clone(),
                framed: framed,
                items: Vec::new(),
                sink_items: VecDeque::new(),
                sink_flushed: true,
            }
        }
    }

    /// Build service for `T` and stream `S`
    #[must_use = "service do nothing unless polled"]
    fn build_from(self, srv: &Service<Self::Context>,
                  framed: Framed<Self::FramedContext, Self::Codec>)
                  -> CtxFramedServiceBuilder<Self>
    {
        CtxFramedServiceBuilder {
            srv: CtxFramedService {
                flags: State::empty(),
                ctx: srv.clone(),
                exec: self,
                handle: srv.handle().clone(),
                framed: framed,
                items: Vec::new(),
                sink_items: VecDeque::new(),
                sink_flushed: true,
            }
        }
    }

    /// Method is called when service get polled first time.
    fn start(&mut self, _ctx: &mut Self::Context, _srv: &mut CtxFramedService<Self>) {}

    /// Method is called when wrapped stream completes.
    fn finished(&mut self, ctx: &mut Self::Context, srv: &mut CtxFramedService<Self>)
                -> Result<Async<<<Self as FramedContextAware>::Result as CtxMessage>::Item>,
                          <<Self as FramedContextAware>::Result as CtxMessage>::Error>;

    fn call(&mut self,
            ctx: &mut Self::Context,
            srv: &mut CtxFramedService<Self>, CtxFramedResult<Self>) ->
        Result<Async<<<Self as FramedContextAware>::Result as CtxMessage>::Item>,
               <Self::Result as CtxMessage>::Error>;
}

pub struct CtxFramedServiceBuilder<T> where T: FramedContextAware {
    srv: CtxFramedService<T>,
}

impl<T> CtxFramedServiceBuilder<T>
    where T: FramedContextAware,
{
    pub fn run(self) where Self: 'static, T: 'static {
        self.srv.run()
    }

    /// Add future
    pub fn add_future<F>(mut self, fut: F) -> Self
        where F: Future<Item=<<T as FramedContextAware>::Codec as Decoder>::Item,
                        Error=<<T as FramedContextAware>::Codec as Decoder>::Error> + 'static
    {
        self.srv.add_future(fut);
        self
    }

    /// Add stream
    pub fn add_stream<S>(mut self, fut: S) -> Self
        where S: Stream<Item=<<T as FramedContextAware>::Codec as Decoder>::Item,
                        Error=<<T as FramedContextAware>::Codec as Decoder>::Error> + 'static
    {
        self.srv.add_stream(fut);
        self
    }

    /// Add stream
    pub fn add_fut_stream<F>(mut self, fut: F) -> Self
        where F: Future<Item=
                        Box<Stream<Item=<<T as FramedContextAware>::Codec as Decoder>::Item,
                                   Error=<<T as FramedContextAware>::Codec as Decoder>::Error>>,
                        Error=<<T as FramedContextAware>::Codec as Decoder>::Error> + 'static
    {
        self.srv.add_fut_stream(fut);
        self
    }
}

pub struct CtxFramedService<T> where T: FramedContextAware
{
    flags: State,
    ctx: Rc<RefCell<T::Context>>,
    handle: Handle,
    exec: T,
    framed: Framed<T::FramedContext, T::Codec>,
    items: Vec<Item<T>>,
    sink_items: VecDeque<<T::Codec as Encoder>::Item>,
    sink_flushed: bool,
}

impl<T> CtxFramedService<T> where T: FramedContextAware,
{
    pub fn run(self) where Self: 'static, T: 'static
    {
        let handle: &Handle = unsafe{std::mem::transmute(&self.handle)};
        handle.spawn(self.map(|_| ()).map_err(|_| ()))
    }

    pub fn send(&mut self, item: <T::Codec as Encoder>::Item)
            -> Result<(), <T::Codec as Encoder>::Item>
    {
        if self.sink_items.is_empty() {
            self.sink_items.push_back(item);
            Ok(())
        } else {
            Err(item)
        }
    }

    pub fn send_buffered(&mut self, item: <T::Codec as Encoder>::Item) {
        self.sink_items.push_back(item);
    }

    pub fn spawn<F>(&mut self, fut: F)
        where F: CtxFuture<Item=(), Error=(), Context=T::Context, Service=Self> + 'static
    {
        self.items.push(Item::CtxSpawnFuture(Box::new(fut)))
    }

    pub fn add_future<F>(&mut self, fut: F)
        where F: Future<Item=<<T as FramedContextAware>::Codec as Decoder>::Item,
                        Error=<<T as FramedContextAware>::Codec as Decoder>::Error> + 'static
    {
        self.items.push(Item::Future(Box::new(fut)))
    }

    pub fn add_stream<S>(&mut self, fut: S)
        where S: Stream<Item=<<T as FramedContextAware>::Codec as Decoder>::Item,
                        Error=<<T as FramedContextAware>::Codec as Decoder>::Error> + 'static
    {
        self.items.push(Item::Stream(Box::new(fut)))
    }

    pub fn add_fut_stream<F>(&mut self, fut: F)
        where F: Future<Item=
                        Box<Stream<Item=<<T as FramedContextAware>::Codec as Decoder>::Item,
                                   Error=<<T as FramedContextAware>::Codec as Decoder>::Error>>,
                        Error=<<T as FramedContextAware>::Codec as Decoder>::Error> + 'static
    {
        self.items.push(Item::FutStream(Box::new(fut)))
    }
}

impl<T> Future for CtxFramedService<T>
    where T: FramedContextAware
{
    type Item = <<T as FramedContextAware>::Result as CtxMessage>::Item;
    type Error = <<T as FramedContextAware>::Result as CtxMessage>::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let srv: &mut CtxFramedService<T> = unsafe {
            std::mem::transmute(self as &mut CtxFramedService<T>)
        };
        let ctx = &mut *self.ctx.borrow_mut();

        if !self.flags.contains(STARTED) {
            self.flags |= STARTED;
            FramedContextAware::start(&mut self.exec, ctx, srv);
        }

        loop {
            let mut not_ready = true;

            match self.framed.poll() {
                Ok(val) => {
                    match val {
                        Async::Ready(Some(val)) => {
                            not_ready = false;
                            match FramedContextAware::call(
                                &mut self.exec, ctx, srv, CtxFramedResult::Ok(val))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                        }
                        Async::Ready(None) =>
                            match FramedContextAware::finished(&mut self.exec, ctx, srv)
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                        Async::NotReady => (),
                    }
                }
                Err(err) => {
                    match FramedContextAware::call(
                        &mut self.exec, ctx, srv, CtxFramedResult::Err(err))
                    {
                        Ok(Async::NotReady) => (),
                        val => return val,
                    }
                }
            }

            // check secondary streams
            let mut idx = 0;
            let mut len = self.items.len();
            loop {
                if idx >= len {
                    break
                }

                let (drop, item, result) = match self.items[idx] {
                    Item::Stream(ref mut stream) => match stream.poll() {
                        Ok(val) => match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                (false, None, Some(FramedContextAware::call(
                                    &mut self.exec, ctx, srv, CtxFramedResult::Ok(val))))
                            }
                            Async::Ready(None) => (true, None, None),
                            Async::NotReady => (false, None, None),
                        }
                        Err(err) => (true, None, Some(FramedContextAware::call(
                            &mut self.exec, ctx, srv, CtxFramedResult::Err(err)))),
                    },
                    Item::FutStream(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => (true, Some(Item::Stream(val)), None),
                            Async::NotReady => (false, None, None),
                        }
                        Err(err) => (true, None, Some(FramedContextAware::call(
                            &mut self.exec, ctx, srv, CtxFramedResult::Err(err)))),
                    }
                    Item::Future(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                (true, None, Some(FramedContextAware::call(
                                    &mut self.exec, ctx, srv, CtxFramedResult::Ok(val))))
                            }
                            Async::NotReady => (false, None, None),
                        }
                        Err(err) => (true, None, Some(FramedContextAware::call(
                            &mut self.exec, ctx, srv, CtxFramedResult::Err(err))))
                    }
                    Item::CtxFuture(ref mut fut) => match fut.poll(ctx, srv) {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                (true, None, Some(FramedContextAware::call(
                                    &mut self.exec, ctx, srv, CtxFramedResult::Ok(val))))
                            }
                            Async::NotReady => (false, None, None),
                        }
                        Err(err) => (true, None, Some(FramedContextAware::call(
                            &mut self.exec, ctx, srv, CtxFramedResult::Err(err)))),
                    }
                    Item::CtxSpawnFuture(ref mut fut) => match fut.poll(ctx, srv) {
                        Ok(val) => match val {
                            Async::Ready(_) => {
                                not_ready = false;
                                (true, None, None)
                            }
                            Async::NotReady => (false, None, None),
                        }
                        Err(_) => (true, None, None)
                    }
                };

                // we have item to add
                if let Some(item) = item {
                    self.items.push(item);
                }

                // number of items could be different, context handler could add more items
                len = self.items.len();

                // move last item to current position
                if drop {
                    len = len - 1;
                    if idx >= len {
                        self.items.pop();
                        match result {
                            None => (),
                            Some(result) => match result {
                                Ok(Async::NotReady) => (),
                                result => return result,
                            }
                        }
                        break
                    } else {
                        self.items[idx] = self.items.pop().unwrap();
                    }
                } else {
                    idx += 1;
                }

                match result {
                    None => (),
                    Some(result) => match result {
                        Ok(Async::NotReady) => (),
                        result => return result,
                    }
                }
            }

            // send sink items
            loop {
                if self.sink_flushed {
                    if let Some(item) = self.sink_items.pop_front() {
                        match self.framed.start_send(item) {
                            Ok(AsyncSink::NotReady(item)) => {
                                self.sink_flushed = false;
                                self.sink_items.push_front(item);
                            }
                            Ok(AsyncSink::Ready) => {
                                if self.sink_items.is_empty() {
                                    self.sink_flushed = false;
                                }
                                continue
                            }
                            Err(err) => {
                                match FramedContextAware::call(
                                    &mut self.exec, ctx, srv, CtxFramedResult::SinkErr(err))
                                {
                                    Ok(Async::NotReady) => break,
                                    val => return val,
                                }
                            }
                        }
                    } else {
                        break
                    }
                } else  {
                    // flush sink
                    match self.framed.poll_complete() {
                        Ok(Async::Ready(_)) => {
                            not_ready = false;
                            self.sink_flushed = true;
                            match FramedContextAware::call(
                                &mut self.exec, ctx, srv, CtxFramedResult::Sent)
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                        }
                        Ok(Async::NotReady) => {
                            break
                        }
                        Err(err) => {
                            match FramedContextAware::call(
                                &mut self.exec, ctx, srv, CtxFramedResult::SinkErr(err))
                            {
                                Ok(Async::NotReady) => break,
                                val => return val,
                            }
                        }
                    };
                }
            }

            // are we done
            if not_ready {
                return Ok(Async::NotReady)
            }
        }
    }
}

impl<T> Service<T::Context> for CtxFramedService<T>
    where T: FramedContextAware
{
    fn handle(&self) -> &Handle {
        &self.handle
    }

    fn clone(&self) -> Rc<RefCell<T::Context>> {
        self.ctx.clone()
    }
}
