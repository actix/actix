#![allow(dead_code)]
use std;
use std::rc::Rc;
use std::cell::RefCell;
use std::borrow::Borrow;

use futures::{self, Async, Future, Poll, Stream};
use tokio_core::reactor::Handle;

use fut::CtxFuture;
use service::{Message, Service};
use sink::{Sink, SinkService, SinkContext, SinkContextService};

/// Service execution context object
pub struct Context<T> where T: Service<Context=Context<T>>,
{
    st: Rc<RefCell<T::State>>,
    srv: T,
    handle: Handle,
    started: bool,
    stream: Box<Stream<Item=<T::Message as Message>::Item,
                       Error=<T::Message as Message>::Error>>,
    items: Vec<Item<T>>,
}

/// io items
enum Item<T: Service> {
    CtxFuture(Box<ServiceCtxFuture<T>>),
    CtxSpawnFuture(Box<ServiceCtxSpawnFuture<T>>),
    Future(Box<ServiceFuture<T>>),
    Stream(Box<ServiceStream<T>>),
    FutStream(Box<ServiceFutStream<T>>),
    Sink(Box<SinkContextService<Service=T>>),
}

type ServiceCtxFuture<T> =
    CtxFuture<Item=<<T as Service>::Message as Message>::Item,
              Error=<<T as Service>::Message as Message>::Error,
              Service=T, Context=<T as Service>::Context>;

type ServiceCtxSpawnFuture<T> =
    CtxFuture<Item=(), Error=(), Service=T, Context=<T as Service>::Context>;

type ServiceFuture<T> =
    Future<Item=<<T as Service>::Message as Message>::Item,
           Error=<<T as Service>::Message as Message>::Error>;

pub type ServiceStream<T> =
    Stream<Item=<<T as Service>::Message as Message>::Item,
           Error=<<T as Service>::Message as Message>::Error>;

type ServiceFutStream<T> =
    Future<Item=Box<ServiceStream<T>>,
           Error=<<T as Service>::Message as Message>::Error>;


impl<T> Context<T> where T: Service<Context=Context<T>>
{
    pub(crate) fn new<S>(st: Rc<RefCell<T::State>>, srv: T, stream: S, handle: &Handle)
                         -> Context<T>
        where S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static,
    {
        Context {
            st: st,
            srv: srv,
            started: false,
            handle: handle.clone(),
            stream: Box::new(stream),
            items: Vec::new(),
        }
    }

    pub(crate) fn set_service(&mut self, srv: T) {
        self.srv = srv
    }

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
        where F: CtxFuture<Item=(), Error=(), Service=T, Context=Self> + 'static
    {
        self.items.push(Item::CtxSpawnFuture(Box::new(fut)))
    }

    pub fn add_future<F>(&mut self, fut: F)
        where F: Future<Item=<<T as Service>::Message as Message>::Item,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        self.items.push(Item::Future(Box::new(fut)))
    }

    pub fn add_stream<S>(&mut self, fut: S)
        where S: Stream<Item=<<T as Service>::Message as Message>::Item,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        self.items.push(Item::Stream(Box::new(fut)))
    }

    pub fn add_fut_stream<F>(&mut self, fut: F)
        where F: Future<Item=Box<Stream<Item=<<T as Service>::Message as Message>::Item,
                                        Error=<<T as Service>::Message as Message>::Error>>,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        self.items.push(Item::FutStream(Box::new(fut)))
    }

    pub fn add_sink<C, S>(&mut self, ctx: C, sink: S) -> Sink<C>
        where C: SinkService<Service=T> + 'static,
              S: futures::Sink<SinkItem=<C::SinkMessage as Message>::Item,
                               SinkError=<C::SinkMessage as Message>::Error> + 'static
    {
        let mut srv = Box::new(SinkContext::new(ctx, sink));
        let psrv = srv.as_mut() as *mut _;
        self.items.push(Item::Sink(srv));

        let sink = Sink::new(psrv);
        sink
    }
}

impl<T> std::convert::AsRef<T::State> for Context<T>
    where T: Service<Context=Context<T>>
{
    fn as_ref(&self) -> &T::State {
        let b: &RefCell<T::State> = self.st.borrow();
        let st = b.borrow();
        unsafe {
            std::mem::transmute(&*st)
        }
    }
}

impl<T> std::convert::AsMut<T::State> for Context<T>
    where T: Service<Context=Context<T>>
{
    fn as_mut(&mut self) -> &mut T::State {
        unsafe {
            std::mem::transmute(&mut *self.st.borrow_mut())
        }
    }
}

impl<T> Future for Context<T> where T: Service<Context=Context<T>>
{
    type Item = <<T as Service>::Result as Message>::Item;
    type Error = <<T as Service>::Result as Message>::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let st: &mut T::State = unsafe {
            std::mem::transmute(&mut *self.st.borrow_mut())
        };
        let srv: &mut Context<T> = unsafe {
            std::mem::transmute(self as &mut Context<T>)
        };
        if !self.started {
            self.started = true;
            Service::start(&mut self.srv, st, srv);
        }

        loop {
            let mut not_ready = true;

            match self.stream.poll() {
                Ok(val) => {
                    match val {
                        Async::Ready(Some(val)) => {
                            not_ready = false;
                            match Service::call(&mut self.srv, st, srv, Ok(val)) {
                                Ok(Async::NotReady) => (),
                                val => return val
                            }
                        }
                        Async::Ready(None) => match Service::finished(&mut self.srv, st, srv)
                        {
                            Ok(Async::NotReady) => (),
                            val => return val
                        }
                        Async::NotReady => (),
                    }
                }
                Err(err) => match Service::call(&mut self.srv, st, srv, Err(err)) {
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
                    Item::Sink(ref mut sink) => match sink.poll(st, &mut self.srv, srv) {
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
                                match Service::call(&mut self.srv, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (false, None)
                            }
                            Async::Ready(None) => (true, None),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => match Service::call(&mut self.srv, st, srv, Err(err))
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
                            match Service::call(&mut self.srv, st, srv, Err(err))
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
                                match Service::call(&mut self.srv, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match Service::call(&mut self.srv, st, srv, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    Item::CtxFuture(ref mut fut) => match fut.poll(&mut self.srv, srv) {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                match Service::call(&mut self.srv, st, srv, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match Service::call(&mut self.srv, st, srv, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    Item::CtxSpawnFuture(ref mut fut) => match fut.poll(&mut self.srv, srv) {
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
