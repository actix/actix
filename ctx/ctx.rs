#![allow(dead_code)]

use std;
use std::rc::Rc;
use std::cell::RefCell;

use futures::{Async, Future, Poll, Stream};
use tokio_core::reactor::Handle;

use fut::CtxFuture;


bitflags!(
    /// State Bitflags
    struct State: u16 {
        /// Service is started
        const STARTED = 0b00000001;
    }
);

/// Context service trait
/// T: The type of item this context work with.
pub trait Service<T> {

    /// Core handle
    fn handle(&self) -> &Handle;

    fn clone(&self) -> Rc<RefCell<T>>;

}

pub trait CtxMessage {
    type Item;
    type Error;
}

impl<T, E> CtxMessage for Result<T, E> {
    type Item=T;
    type Error=E;
}

pub trait ContextAware: Sized + 'static {

    type State;
    type Message: CtxMessage;
    type Result: CtxMessage;

    /// Create new context for `Context` and stream `S` and run
    fn run<S>(self, st: Self::State, stream: S, handle: &Handle)
        where S: Stream<Item=<<Self as ContextAware>::Message as CtxMessage>::Item,
                        Error=<<Self as ContextAware>::Message as CtxMessage>::Error> + 'static,
    {
        CtxService {
            st: Rc::new(RefCell::new(st)),
            flags: State::empty(),
            handle: handle.clone(),
            ctx: self,
            stream: Box::new(stream),
            items: Vec::new(),
        }.run()
    }

    /// Create new context for `Context` and stream `S`
    fn clone_and_run<S>(self, st: Self::State, stream: S, handle: &Handle)
                        -> Rc<RefCell<Self::State>>
        where S: Stream<Item=<<Self as ContextAware>::Message as CtxMessage>::Item,
                        Error=<<Self as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        let srv = CtxService {
            st: Rc::new(RefCell::new(st)),
            flags: State::empty(),
            handle: handle.clone(),
            ctx: self,
            stream: Box::new(stream),
            items: Vec::new(),
        };
        let st = srv.clone();
        srv.run();
        st
    }

    /// Spawn shared context for stream `S`
    fn spawn<S>(self, srv: &Service<Self::State>, stream: S)
        where S: Stream<Item=<Self::Message as CtxMessage>::Item,
                        Error=<Self::Message as CtxMessage>::Error> + 'static,
    {
        CtxService {
            st: srv.clone(),
            flags: State::empty(),
            handle: srv.handle().clone(),
            ctx: self,
            stream: Box::new(stream),
            items: Vec::new(),
        }.run()
    }

    /// Build service for `T` and stream `S`
    #[must_use = "service do nothing unless polled"]
    fn build<S>(self, st: Self::State, stream: S, handle: &Handle) -> CtxServiceBuilder<Self>
        where S: Stream<Item=<<Self as ContextAware>::Message as CtxMessage>::Item,
                        Error=<<Self as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        CtxServiceBuilder {
            srv: CtxService {
                st: Rc::new(RefCell::new(st)),
                flags: State::empty(),
                ctx: self,
                handle: handle.clone(),
                stream: Box::new(stream),
                items: Vec::new(),
            }
        }
    }

    /// Build service for `T` and stream `S`
    #[must_use = "service do nothing unless polled"]
    fn build_from<S>(self, srv: &Service<Self::State>, stream: S) -> CtxServiceBuilder<Self>
        where S: Stream<Item=<<Self as ContextAware>::Message as CtxMessage>::Item,
                        Error=<<Self as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        CtxServiceBuilder {
            srv: CtxService {
                st: srv.clone(),
                flags: State::empty(),
                ctx: self,
                handle: srv.handle().clone(),
                stream: Box::new(stream),
                items: Vec::new(),
            }
        }
    }

    /// Method is called when service get polled first time.
    fn start(&mut self, _ctx: &mut Self::State, _srv: &mut CtxService<Self>) {}

    /// Method is called when wrapped stream completes.
    fn finished(&mut self, ctx: &mut Self::State, srv: &mut CtxService<Self>)
                -> Result<Async<<<Self as ContextAware>::Result as CtxMessage>::Item>,
                          <<Self as ContextAware>::Result as CtxMessage>::Error>;

    /// Method is called for every item from stream.
    fn call(&mut self,
            st: &mut Self::State,
            srv: &mut CtxService<Self>,
            result: Result<<Self::Message as CtxMessage>::Item,
                           <Self::Message as CtxMessage>::Error>)
            -> Result<Async<<<Self as ContextAware>::Result as CtxMessage>::Item>,
                      <<Self as ContextAware>::Result as CtxMessage>::Error>;
}

pub struct CtxServiceBuilder<T> where T: ContextAware {
    srv: CtxService<T>,
}

impl<T> CtxServiceBuilder<T> where T: ContextAware,
{
    pub fn run(self) where Self: 'static, T: 'static {
        self.srv.run()
    }

    pub fn clone_and_run(self) -> Rc<RefCell<T::State>> where Self: 'static, T: 'static
    {
        let st = self.srv.clone();
        self.srv.run();
        st
    }

    /// Add future
    pub fn add_future<F>(mut self, fut: F) -> Self
        where F: Future<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
                        Error=<<T as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        self.srv.add_future(fut);
        self
    }

    /// Add stream
    pub fn add_stream<S>(mut self, fut: S) -> Self
        where S: Stream<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
                        Error=<<T as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        self.srv.add_stream(fut);
        self
    }

    /// Add stream
    pub fn add_fut_stream<F>(mut self, fut: F) -> Self
        where F: Future<Item=
                        Box<Stream<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
                                   Error=<<T as ContextAware>::Message as CtxMessage>::Error>>,
                        Error=<<T as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        self.srv.add_fut_stream(fut);
        self
    }
}

/// io items
enum Item<T: ContextAware> {
    CtxFuture(Box<CtxServiceCtxFuture<T>>),
    CtxSpawnFuture(Box<CtxServiceCtxSpawnFuture<T>>),
    Future(Box<CtxServiceFuture<T>>),
    Stream(Box<CtxServiceStream<T>>),
    FutStream(Box<CtxServiceFutStream<T>>),
}

type CtxServiceCtxFuture<T> =
    CtxFuture<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
              Error=<<T as ContextAware>::Message as CtxMessage>::Error,
              Context=<T as ContextAware>::State, Service=CtxService<T>>;

type CtxServiceCtxSpawnFuture<T> =
    CtxFuture<Item=(), Error=(),
              Context=<T as ContextAware>::State, Service=CtxService<T>>;

type CtxServiceFuture<T> =
    Future<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
           Error=<<T as ContextAware>::Message as CtxMessage>::Error>;

pub type CtxServiceStream<T> =
    Stream<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
           Error=<<T as ContextAware>::Message as CtxMessage>::Error>;

type CtxServiceFutStream<T> =
    Future<Item=Box<CtxServiceStream<T>>,
           Error=<<T as ContextAware>::Message as CtxMessage>::Error>;


pub struct CtxService<T> where T: ContextAware,
{
    st: Rc<RefCell<T::State>>,
    flags: State,
    handle: Handle,
    ctx: T,
    stream: Box<Stream<Item=<T::Message as CtxMessage>::Item,
                       Error=<T::Message as CtxMessage>::Error>>,
    items: Vec<Item<T>>,
}

impl<T> CtxService<T> where T: ContextAware
{
    pub fn run(self) where T: 'static
    {
        let handle: &Handle = unsafe{std::mem::transmute(&self.handle)};
        handle.spawn(self.map(|_| ()).map_err(|_| ()))
    }

    pub fn spawn<F>(&mut self, fut: F)
        where F: CtxFuture<Item=(), Error=(), Context=T::State, Service=Self> + 'static
    {
        self.items.push(Item::CtxSpawnFuture(Box::new(fut)))
    }

    pub fn add_future<F>(&mut self, fut: F)
        where F: Future<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
                        Error=<<T as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        self.items.push(Item::Future(Box::new(fut)))
    }

    pub fn add_stream<S>(&mut self, fut: S)
        where S: Stream<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
                        Error=<<T as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        self.items.push(Item::Stream(Box::new(fut)))
    }

    pub fn add_fut_stream<F>(&mut self, fut: F)
        where F: Future<Item=
                        Box<Stream<Item=<<T as ContextAware>::Message as CtxMessage>::Item,
                                   Error=<<T as ContextAware>::Message as CtxMessage>::Error>>,
                        Error=<<T as ContextAware>::Message as CtxMessage>::Error> + 'static
    {
        self.items.push(Item::FutStream(Box::new(fut)))
    }
}

impl<T> Service<T::State> for CtxService<T> where T: ContextAware
{
    fn handle(&self) -> &Handle {
        &self.handle
    }

    fn clone(&self) -> Rc<RefCell<T::State>> {
        self.st.clone()
    }
}


impl<T> Future for CtxService<T> where T: ContextAware
{
    type Item = <<T as ContextAware>::Result as CtxMessage>::Item;
    type Error = <<T as ContextAware>::Result as CtxMessage>::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let srv: &mut CtxService<T> = unsafe {
            std::mem::transmute(self as &mut CtxService<T>)
        };
        let st = &mut *self.st.borrow_mut();

        if !self.flags.contains(STARTED) {
            self.flags |= STARTED;
            ContextAware::start(&mut self.ctx, st, srv);
        }

        loop {
            let mut not_ready = true;

            match self.stream.poll() {
                Ok(val) => {
                    match val {
                        Async::Ready(Some(val)) => {
                            not_ready = false;
                            match ContextAware::call(&mut self.ctx, st, srv, Ok(val))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val
                            }
                        }
                        Async::Ready(None) => {
                            match ContextAware::finished(&mut self.ctx, st, srv)
                            {
                                Ok(Async::NotReady) => (),
                                val => return val
                            }
                            continue
                        }
                        Async::NotReady => (),
                    }
                }
                Err(err) => {
                    match ContextAware::call(&mut self.ctx, st, srv, Err(err))
                    {
                        Ok(Async::NotReady) => (),
                        val => return val,
                    }
                    continue
                }
            }

            // check secondary streams
            let mut idx = 0;
            let mut len = self.items.len();
            loop {
                if idx >= len {
                    break
                }

                let (drop, item) = match self.items[idx] {
                    Item::Stream(ref mut stream) => match stream.poll() {
                        Ok(val) => match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                match ContextAware::call(&mut self.ctx, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (false, None)
                            }
                            Async::Ready(None) => (true, None),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match ContextAware::call(&mut self.ctx, st, srv, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    },
                    Item::FutStream(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => (true, Some(Item::Stream(val))),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match ContextAware::call(&mut self.ctx, st, srv, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    Item::Future(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                match ContextAware::call(&mut self.ctx, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match ContextAware::call(&mut self.ctx, st, srv, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    Item::CtxFuture(ref mut fut) => match fut.poll(st, srv) {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                match ContextAware::call(&mut self.ctx, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match ContextAware::call(&mut self.ctx, st, srv, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    Item::CtxSpawnFuture(ref mut fut) => match fut.poll(st, srv) {
                        Ok(val) => match val {
                            Async::Ready(_) => {
                                not_ready = false;
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(_) => (true, None)
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
                        break
                    } else {
                        self.items[idx] = self.items.pop().unwrap();
                    }
                } else {
                    idx += 1;
                }
            }

            // are we done
            if not_ready {
                return Ok(Async::NotReady)
            }
        }
    }
}
