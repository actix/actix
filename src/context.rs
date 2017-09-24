#![allow(dead_code)]
use std;

use futures::{self, Async, Future, Poll, Stream};
use futures::unsync::mpsc::{unbounded, UnboundedReceiver};
use tokio_core::reactor::Handle;

use service::{Item, Service};

use fut::CtxFuture;
use address::{Address, BoxedMessageProxy};
use sink::{Sink, SinkService, SinkContext, SinkContextService};

#[macro_export]
macro_rules! try_service {
    ($e:expr) => (match $e {
        $crate::ServiceResult::Done => return Ok($crate::context::Async::Ready(())),
        $crate::ServiceResult::NotReady => (),
    })
}

bitflags! {
    /// State Bitflags
    struct State: u16 {
        /// Service is started
        const STARTED = 0b00000001;
        /// Service is done
        const DONE = 0b00000010;
    }
}

/// Service execution context object
pub struct Context<T> where T: Service<Context=Context<T>>,
{
    srv: T,
    addr: Address<T>,
    handle: Handle,
    flags: State,
    msgs: UnboundedReceiver<BoxedMessageProxy<T>>,
    items: Vec<IoItem<T>>,
    stream: Option<Box<Stream<Item=<T::Message as Item>::Item,
                              Error=<T::Message as Item>::Error>>>,
}

/// io items
enum IoItem<T: Service> {
    CtxFuture(Box<ServiceCtxFuture<T>>),
    CtxSpawnFuture(Box<ServiceCtxSpawnFuture<T>>),
    Future(Box<ServiceFuture<T>>),
    Stream(Box<ServiceStream<T>>),
    Sink(Box<SinkContextService<Service=T>>),
}

type ServiceCtxFuture<T> =
    CtxFuture<Item=<<T as Service>::Message as Item>::Item,
              Error=<<T as Service>::Message as Item>::Error,
              Service=T>;

type ServiceCtxSpawnFuture<T> =
    CtxFuture<Item=(), Error=(), Service=T>;

type ServiceFuture<T> =
    Future<Item=<<T as Service>::Message as Item>::Item,
           Error=<<T as Service>::Message as Item>::Error>;

pub type ServiceStream<T> =
    Stream<Item=<<T as Service>::Message as Item>::Item,
           Error=<<T as Service>::Message as Item>::Error>;


impl<T> Context<T> where T: Service<Context=Context<T>>
{
    pub(crate) fn new<S>(srv: T, stream: S, handle: &Handle) -> Context<T>
        where S: Stream<Item=<T::Message as Item>::Item,
                        Error=<T::Message as Item>::Error> + 'static,
    {
        let (tx, rx) = unbounded();

        Context {
            srv: srv,
            msgs: rx,
            addr: Address::new(tx),
            flags: State::empty(),
            handle: handle.clone(),
            stream: Some(Box::new(stream)),
            items: Vec::new(),
        }
    }

    pub(crate) fn new_empty(srv: T, handle: &Handle) -> Context<T>
    {
        let (tx, rx) = unbounded();
        Context {
            srv: srv,
            msgs: rx,
            addr: Address::new(tx),
            flags: State::empty(),
            handle: handle.clone(),
            stream: None,
            items: Vec::new(),
        }
    }

    pub(crate) fn run(self) where T: 'static {
        let handle: &Handle = unsafe{std::mem::transmute(&self.handle)};
        handle.spawn(self.map(|_| ()).map_err(|_| ()))
    }

    pub(crate) fn replace_service(&mut self, srv: T) -> T {
        std::mem::replace(&mut self.srv, srv)
    }

    pub fn address(&self) -> Address<T> {
        self.addr.clone()
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn set_done(&mut self) {
        self.flags |= State::DONE;
    }

    pub fn spawn<F>(&mut self, fut: F)
        where F: CtxFuture<Item=(), Error=(), Service=T> + 'static
    {
        self.items.push(IoItem::CtxSpawnFuture(Box::new(fut)))
    }

    pub fn add_future<F>(&mut self, fut: F)
        where F: Future<Item=<<T as Service>::Message as Item>::Item,
                        Error=<<T as Service>::Message as Item>::Error> + 'static
    {
        self.items.push(IoItem::Future(Box::new(fut)))
    }

    pub fn add_stream<S>(&mut self, fut: S)
        where S: Stream<Item=<<T as Service>::Message as Item>::Item,
                        Error=<<T as Service>::Message as Item>::Error> + 'static
    {
        self.items.push(IoItem::Stream(Box::new(fut)))
    }

    pub fn add_sink<C, S>(&mut self, ctx: C, sink: S) -> Sink<C>
        where C: SinkService<Service=T> + 'static,
              S: futures::Sink<SinkItem=<C::SinkMessage as Item>::Item,
                               SinkError=<C::SinkMessage as Item>::Error> + 'static
    {
        let mut srv = Box::new(SinkContext::new(ctx, sink));
        let psrv = srv.as_mut() as *mut _;
        self.items.push(IoItem::Sink(srv));

        let sink = Sink::new(psrv);
        sink
    }
}

impl<T> Future for Context<T> where T: Service<Context=Context<T>>
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut Context<T> = unsafe {
            std::mem::transmute(self as &mut Context<T>)
        };
        if !self.flags.contains(State::STARTED) {
            self.flags |= State::STARTED;
            Service::start(&mut self.srv, ctx);
        }

        loop {
            let mut not_ready = true;

            if let Some(ref mut stream) = self.stream {
                match stream.poll() {
                    Ok(val) => {
                        match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                try_service!(Service::call(&mut self.srv, ctx, Ok(val)));
                            }
                            Async::Ready(None) =>
                                try_service!(Service::finished(&mut self.srv, ctx)),
                            Async::NotReady => (),
                        }
                    }
                    Err(err) => try_service!(Service::call(&mut self.srv, ctx, Err(err)))
                }
            }

            // check messages
            match self.msgs.poll() {
                Ok(val) => {
                    match val {
                        Async::Ready(Some(mut msg)) => {
                            not_ready = false;
                            msg.handle(&mut self.srv, ctx);
                        }
                        Async::Ready(None) => (),
                        Async::NotReady => (),
                    }
                }
                Err(_) => (),
            }

            // check secondary streams
            let mut idx = 0;
            let mut len = self.items.len();
            loop {
                if idx >= len {
                    break
                }

                let (drop, item) = match self.items[idx] {
                    IoItem::Sink(ref mut sink) => {
                        try_service!(sink.poll(&mut self.srv, ctx));
                        (false, None)
                    }
                    IoItem::Stream(ref mut stream) => match stream.poll() {
                        Ok(val) => match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                try_service!(Service::call(&mut self.srv, ctx, Ok(val)));
                                (false, None)
                            }
                            Async::Ready(None) => (true, None),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            try_service!(Service::call(&mut self.srv, ctx, Err(err)));
                            (true, None)
                        }
                    },
                    IoItem::Future(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                try_service!(Service::call(&mut self.srv, ctx, Ok(val)));
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            try_service!(Service::call(&mut self.srv, ctx, Err(err)));
                            (true, None)
                        }
                    }
                    IoItem::CtxFuture(ref mut fut) => match fut.poll(&mut self.srv, ctx) {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                try_service!(Service::call(&mut self.srv, ctx, Ok(val)));
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            try_service!(Service::call(&mut self.srv, ctx, Err(err)));
                            (true, None)
                        }
                    }
                    IoItem::CtxSpawnFuture(ref mut fut) => match fut.poll(&mut self.srv, ctx) {
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

            if self.flags.contains(State::DONE) {
                return Ok(Async::Ready(()))
            }

            // are we done
            if not_ready {
                return Ok(Async::NotReady)
            }
        }
    }
}


pub trait CtxFutureSpawner<S> where S: Service<Context=Context<S>> {

    /// spawn future into Context
    fn spawn(self, fut: &mut Context<S>);

}


impl<T, S> CtxFutureSpawner<S> for T
    where S: Service<Context=Context<S>>,
          T: CtxFuture<Item=(), Error=(), Service=S> + 'static
{
    fn spawn(self, ctx: &mut Context<S>) {
        ctx.spawn(self)
    }

}
