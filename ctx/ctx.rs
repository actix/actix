#![allow(dead_code)]

use std;
use std::rc::Rc;
use std::cell::RefCell;
use std::borrow::{Borrow};

use boxfnonce::BoxFnOnce;
use futures::{future, Async, Future, Poll, Stream, Sink};
use tokio_core::reactor::Handle;

use fut::CtxFuture;
use sink::{SinkContext, CtxSink, CtxSinkService, CtxSinkContextService};


pub trait Message {
    type Item;
    type Error;
}

impl<T, E> Message for Result<T, E> {
    type Item=T;
    type Error=E;
}

pub trait CtxContext: Sized + 'static {

    type State;
    type Message: Message;
    type Result: Message;

    /// Create new context for `Context` and stream `S` and run
    fn run<S>(self, st: Self::State, stream: S, handle: &Handle)
        where S: Stream<Item=<<Self as CtxContext>::Message as Message>::Item,
                        Error=<<Self as CtxContext>::Message as Message>::Error> + 'static,
    {
        CtxService {
            st: Rc::new(RefCell::new(st)),
            started: false,
            handle: handle.clone(),
            ctx: self,
            stream: Box::new(stream),
            items: Vec::new(),
        }.run()
    }

    /// Create new context for `Context` and stream `S`
    fn clone_and_run<S>(self, st: Self::State, stream: S, handle: &Handle)
                        -> Rc<RefCell<Self::State>>
        where S: Stream<Item=<<Self as CtxContext>::Message as Message>::Item,
                        Error=<<Self as CtxContext>::Message as Message>::Error> + 'static
    {
        let srv = CtxService {
            st: Rc::new(RefCell::new(st)),
            started: false,
            handle: handle.clone(),
            ctx: self,
            stream: Box::new(stream),
            items: Vec::new(),
        };
        let st = srv.clone();
        srv.run();
        st
    }

    /// Method is called when service get polled first time.
    fn start(&mut self, _ctx: &mut Self::State, _srv: &mut CtxService<Self>) {}

    /// Method is called when wrapped stream finishes.
    fn finished(&mut self, ctx: &mut Self::State, srv: &mut CtxService<Self>)
                -> Poll<<<Self as CtxContext>::Result as Message>::Item,
                        <<Self as CtxContext>::Result as Message>::Error>;

    /// Method is called for every item from stream.
    fn call(&mut self,
            st: &mut Self::State,
            srv: &mut CtxService<Self>,
            result: Result<<Self::Message as Message>::Item,
                           <Self::Message as Message>::Error>)
            -> Poll<<<Self as CtxContext>::Result as Message>::Item,
                    <<Self as CtxContext>::Result as Message>::Error>;
}

pub struct CtxBuilder<T> where T: CtxContext {
    srv: CtxService<T>,
    factory: Option<BoxFnOnce<(CtxService<T>,)>>,
}

impl<T> CtxBuilder<T> where T: CtxContext
{
    /// Build service for `T` and stream `S`
    // #[must_use = "service do nothing unless polled"]
    pub fn new<S>(ctx: T, st: T::State, stream: S, handle: &Handle) -> Self
        where S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static,
    {
        CtxBuilder {
            srv: CtxService {
                st: Rc::new(RefCell::new(st)),
                started: false,
                handle: handle.clone(),
                ctx: ctx,
                stream: Box::new(stream),
                items: Vec::new(),
            },
            factory: None}
    }

    /// Build service for `T` and stream `S`
    // #[must_use = "service do nothing unless polled"]
    pub fn build<S, F>(st: T::State, stream: S, handle: &Handle, f: F) -> Self
        where F: 'static + FnOnce(&mut CtxService<T>) -> T,
              S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static,

    {
        CtxBuilder {
            srv: CtxService {
                st: Rc::new(RefCell::new(st)),
                started: false,
                handle: handle.clone(),
                ctx: unsafe{std::mem::uninitialized()},
                stream: Box::new(stream),
                items: Vec::new(),
            },
            factory: Some(BoxFnOnce::from(|mut srv| {
                let ctx = f(&mut srv);
                srv.ctx = ctx;
                srv.run();
            }))
        }
    }

    /// Build service for `T` and stream `S`
    // #[must_use = "service do nothing unless polled"]
    pub fn from_srv<C, S, F>(srv: &CtxService<C>, stream: S, f: F) -> Self
        where C: CtxContext<State=T::State>,
              F: FnOnce(&mut CtxService<T>) -> T + 'static,
              S: Stream<Item=<<T as CtxContext>::Message as Message>::Item,
                        Error=<<T as CtxContext>::Message as Message>::Error> + 'static
    {
        CtxBuilder {
            srv: CtxService {
                st: srv.clone(),
                started: false,
                ctx: unsafe{std::mem::uninitialized()},
                handle: srv.handle().clone(),
                stream: Box::new(stream),
                items: Vec::new(),
            },
            factory: Some(BoxFnOnce::from(|mut srv| {
                let ctx = f(&mut srv);
                srv.ctx = ctx;
                srv.run();
            }))
        }
    }

    pub fn run(self) where Self: 'static, T: 'static {
        let handle: &Handle = unsafe{std::mem::transmute(&self.srv.handle)};
        if let None = self.factory {
            self.srv.run()
        } else {
            handle.spawn_fn(move || {
                let CtxBuilder { srv, factory } = self;
                factory.unwrap().call(srv);
                future::ok(())
            })
        }
    }

    pub fn clone_and_run(self) -> Rc<RefCell<T::State>> where Self: 'static, T: 'static
    {
        let st = self.srv.clone();
        self.srv.run();
        st
    }

    /// Add future
    // #[must_use = "service do nothing unless polled"]
    pub fn add_future<F>(mut self, fut: F) -> Self
        where F: Future<Item=<<T as CtxContext>::Message as Message>::Item,
                        Error=<<T as CtxContext>::Message as Message>::Error> + 'static
    {
        self.srv.add_future(fut);
        self
    }

    /// Add stream
    // #[must_use = "service do nothing unless polled"]
    pub fn add_stream<S>(mut self, fut: S) -> Self
        where S: Stream<Item=<<T as CtxContext>::Message as Message>::Item,
                        Error=<<T as CtxContext>::Message as Message>::Error> + 'static
    {
        self.srv.add_stream(fut);
        self
    }

    /// Add stream
    // #[must_use = "service do nothing unless polled"]
    pub fn add_fut_stream<F>(mut self, fut: F) -> Self
        where F: Future<Item=
                        Box<Stream<Item=<<T as CtxContext>::Message as Message>::Item,
                                   Error=<<T as CtxContext>::Message as Message>::Error>>,
                        Error=<<T as CtxContext>::Message as Message>::Error> + 'static
    {
        self.srv.add_fut_stream(fut);
        self
    }
}

/// io items
enum Item<T: CtxContext> {
    CtxFuture(Box<CtxServiceCtxFuture<T>>),
    CtxSpawnFuture(Box<CtxServiceCtxSpawnFuture<T>>),
    Future(Box<CtxServiceFuture<T>>),
    Stream(Box<CtxServiceStream<T>>),
    FutStream(Box<CtxServiceFutStream<T>>),
    Sink(Box<CtxSinkContextService<Context=T>>),
}

type CtxServiceCtxFuture<T> =
    CtxFuture<Item=<<T as CtxContext>::Message as Message>::Item,
              Error=<<T as CtxContext>::Message as Message>::Error,
              Context=T, Service=CtxService<T>>;

type CtxServiceCtxSpawnFuture<T> =
    CtxFuture<Item=(), Error=(), Context=T, Service=CtxService<T>>;

type CtxServiceFuture<T> =
    Future<Item=<<T as CtxContext>::Message as Message>::Item,
           Error=<<T as CtxContext>::Message as Message>::Error>;

pub type CtxServiceStream<T> =
    Stream<Item=<<T as CtxContext>::Message as Message>::Item,
           Error=<<T as CtxContext>::Message as Message>::Error>;

type CtxServiceFutStream<T> =
    Future<Item=Box<CtxServiceStream<T>>,
           Error=<<T as CtxContext>::Message as Message>::Error>;


pub struct CtxService<T> where T: CtxContext,
{
    st: Rc<RefCell<T::State>>,
    ctx: T,
    handle: Handle,
    started: bool,
    stream: Box<Stream<Item=<T::Message as Message>::Item,
                       Error=<T::Message as Message>::Error>>,
    items: Vec<Item<T>>,
}

impl<T> CtxService<T> where T: CtxContext
{
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn clone(&self) -> Rc<RefCell<T::State>> {
        self.st.clone()
    }

    pub fn run(self) where T: 'static
    {
        let handle: &Handle = unsafe{std::mem::transmute(&self.handle)};
        handle.spawn(self.map(|_| ()).map_err(|_| ()))
    }

    pub fn spawn<F>(&mut self, fut: F)
        where F: CtxFuture<Item=(), Error=(), Context=T, Service=Self> + 'static
    {
        self.items.push(Item::CtxSpawnFuture(Box::new(fut)))
    }

    pub fn add_future<F>(&mut self, fut: F)
        where F: Future<Item=<<T as CtxContext>::Message as Message>::Item,
                        Error=<<T as CtxContext>::Message as Message>::Error> + 'static
    {
        self.items.push(Item::Future(Box::new(fut)))
    }

    pub fn add_stream<S>(&mut self, fut: S)
        where S: Stream<Item=<<T as CtxContext>::Message as Message>::Item,
                        Error=<<T as CtxContext>::Message as Message>::Error> + 'static
    {
        self.items.push(Item::Stream(Box::new(fut)))
    }

    pub fn add_fut_stream<F>(&mut self, fut: F)
        where F: Future<Item=Box<Stream<Item=<<T as CtxContext>::Message as Message>::Item,
                                        Error=<<T as CtxContext>::Message as Message>::Error>>,
                        Error=<<T as CtxContext>::Message as Message>::Error> + 'static
    {
        self.items.push(Item::FutStream(Box::new(fut)))
    }

    pub fn add_sink<C, S>(&mut self, ctx: C, sink: S) -> CtxSink<C>
        where C: SinkContext<Context=T> + 'static,
              S: Sink<SinkItem=<C::SinkMessage as Message>::Item,
                      SinkError=<C::SinkMessage as Message>::Error> + 'static
    {
        let mut srv = Box::new(CtxSinkService::new(ctx, sink));
        let psrv = srv.as_mut() as *mut _;
        self.items.push(Item::Sink(srv));

        let sink = CtxSink::new(psrv);
        sink
    }
}

impl<T> std::convert::AsRef<T::State> for CtxService<T> where T: CtxContext {

    fn as_ref(&self) -> &T::State {
        let b: &RefCell<T::State> = self.st.borrow();
        let st = b.borrow();
        unsafe {
            std::mem::transmute(&*st)
        }
    }
}

impl<T> std::convert::AsMut<T::State> for CtxService<T> where T: CtxContext {

    fn as_mut(&mut self) -> &mut T::State {
        unsafe {
            std::mem::transmute(&mut *self.st.borrow_mut())
        }
    }
}

impl<T> Future for CtxService<T> where T: CtxContext
{
    type Item = <<T as CtxContext>::Result as Message>::Item;
    type Error = <<T as CtxContext>::Result as Message>::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let st: &mut T::State = unsafe {
            std::mem::transmute(&mut *self.st.borrow_mut())
        };
        let srv: &mut CtxService<T> = unsafe {
            std::mem::transmute(self as &mut CtxService<T>)
        };
        if !self.started {
            self.started = true;
            CtxContext::start(&mut self.ctx, st, srv);
        }

        loop {
            let mut not_ready = true;

            match self.stream.poll() {
                Ok(val) => {
                    match val {
                        Async::Ready(Some(val)) => {
                            not_ready = false;
                            match CtxContext::call(&mut self.ctx, st, srv, Ok(val)) {
                                Ok(Async::NotReady) => (),
                                val => return val
                            }
                        }
                        Async::Ready(None) => match CtxContext::finished(&mut self.ctx, st, srv)
                        {
                            Ok(Async::NotReady) => (),
                            val => return val
                        }
                        Async::NotReady => (),
                    }
                }
                Err(err) => match CtxContext::call(&mut self.ctx, st, srv, Err(err)) {
                    Ok(Async::NotReady) => (),
                    val => return val,
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
                    Item::Sink(ref mut sink) => match sink.poll(st, &mut self.ctx, srv) {
                        Ok(val) => match val {
                            Async::Ready(val) => return Ok(Async::Ready(val)),
                            Async::NotReady => (false, None),
                        }
                        other => return other,
                    }
                    Item::Stream(ref mut stream) => match stream.poll() {
                        Ok(val) => match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                match CtxContext::call(&mut self.ctx, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (false, None)
                            }
                            Async::Ready(None) => (true, None),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => match CtxContext::call(&mut self.ctx, st, srv, Err(err))
                        {
                            Ok(Async::NotReady) => (true, None),
                            val => return val,
                        }
                    },
                    Item::FutStream(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => (true, Some(Item::Stream(val))),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match CtxContext::call(&mut self.ctx, st, srv, Err(err))
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
                                match CtxContext::call(&mut self.ctx, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match CtxContext::call(&mut self.ctx, st, srv, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    Item::CtxFuture(ref mut fut) => match fut.poll(&mut self.ctx, srv) {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                match CtxContext::call(&mut self.ctx, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match CtxContext::call(&mut self.ctx, st, srv, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    Item::CtxSpawnFuture(ref mut fut) => match fut.poll(&mut self.ctx, srv) {
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

                // we have new pollable item
                if let Some(item) = item {
                    self.items.push(item);
                }

                // number of items could be different, context can add more items
                len = self.items.len();

                // item finishes, we need to remove it,
                // replace current item with last item
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
