#![allow(dead_code)]
use std;

use futures::{self, Async, Future, Poll, Stream};
use futures::unsync::mpsc::{unbounded, UnboundedReceiver};
use tokio_core::reactor::Handle;

use fut::CtxFuture;
use address::{Address, BoxedMessageProxy};
use service::{Item, Service};
use sink::{Sink, SinkService, SinkContext, SinkContextService};

/// Service execution context object
pub struct Context<T> where T: Service<Context=Context<T>>,
{
    srv: T,
    addr: Address<T>,
    handle: Handle,
    started: bool,
    msgs: UnboundedReceiver<BoxedMessageProxy<T>>,
    items: Vec<IoItem<T>>,
    stream: Box<Stream<Item=<T::Message as Item>::Item, Error=<T::Message as Item>::Error>>,
    result: Option<Result<<<T as Service>::Result as Item>::Item,
                          <<T as Service>::Result as Item>::Error>>,
}

/// io items
enum IoItem<T: Service> {
    CtxFuture(Box<ServiceCtxFuture<T>>),
    CtxSpawnFuture(Box<ServiceCtxSpawnFuture<T>>),
    Future(Box<ServiceFuture<T>>),
    Stream(Box<ServiceStream<T>>),
    FutStream(Box<ServiceFutStream<T>>),
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

type ServiceFutStream<T> =
    Future<Item=Box<ServiceStream<T>>,
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
            started: false,
            handle: handle.clone(),
            stream: Box::new(stream),
            items: Vec::new(),
            result: None,
        }
    }

    pub(crate) fn run(self) where T: 'static {
        let handle: &Handle = unsafe{std::mem::transmute(&self.handle)};
        handle.spawn(self.map(|_| ()).map_err(|_| ()))
    }

    pub(crate) fn set_service(&mut self, srv: T) {
        let old = std::mem::replace(&mut self.srv, srv);
        std::mem::forget(old);
    }

    pub fn address(&self) -> Address<T> {
        self.addr.clone()
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn set_result(&mut self, result: Result<<<T as Service>::Result as Item>::Item,
                                                <<T as Service>::Result as Item>::Error>) {
        self.result = Some(result);
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

    pub fn add_fut_stream<F>(&mut self, fut: F)
        where F: Future<Item=Box<Stream<Item=<<T as Service>::Message as Item>::Item,
                                        Error=<<T as Service>::Message as Item>::Error>>,
                        Error=<<T as Service>::Message as Item>::Error> + 'static
    {
        self.items.push(IoItem::FutStream(Box::new(fut)))
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
    type Item = <<T as Service>::Result as Item>::Item;
    type Error = <<T as Service>::Result as Item>::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut Context<T> = unsafe {
            std::mem::transmute(self as &mut Context<T>)
        };
        if !self.started {
            self.started = true;
            Service::start(&mut self.srv, ctx);
        }

        loop {
            let mut not_ready = true;

            match self.stream.poll() {
                Ok(val) => {
                    match val {
                        Async::Ready(Some(val)) => {
                            not_ready = false;
                            match Service::call(&mut self.srv, ctx, Ok(val)) {
                                Ok(Async::NotReady) => (),
                                val => return val
                            }
                        }
                        Async::Ready(None) => match Service::finished(&mut self.srv, ctx)
                        {
                            Ok(Async::NotReady) => (),
                            val => return val
                        }
                        Async::NotReady => (),
                    }
                }
                Err(err) => match Service::call(&mut self.srv, ctx, Err(err)) {
                    Ok(Async::NotReady) => (),
                    val => return val,
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
                    IoItem::Sink(ref mut sink) => match sink.poll(&mut self.srv, ctx) {
                        Ok(val) => match val {
                            Async::Ready(val) => return Ok(Async::Ready(val)),
                            Async::NotReady => (false, None),
                        }
                        other => return other,
                    }
                    IoItem::Stream(ref mut stream) => match stream.poll() {
                        Ok(val) => match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                match Service::call(&mut self.srv, ctx, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (false, None)
                            }
                            Async::Ready(None) => (true, None),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => match Service::call(&mut self.srv, ctx, Err(err))
                        {
                            Ok(Async::NotReady) => (true, None),
                            val => return val,
                        }
                    },
                    IoItem::FutStream(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => (true, Some(IoItem::Stream(val))),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match Service::call(&mut self.srv, ctx, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    IoItem::Future(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                match Service::call(&mut self.srv, ctx, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match Service::call(&mut self.srv, ctx, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
                            (true, None)
                        }
                    }
                    IoItem::CtxFuture(ref mut fut) => match fut.poll(&mut self.srv, ctx) {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                match Service::call(&mut self.srv, ctx, Ok(val))
                                {
                                    Ok(Async::NotReady) => (),
                                    val => return val,
                                }
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            match Service::call(&mut self.srv, ctx, Err(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
                            }
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

            // check result
            if let Some(result) = self.result.take() {
                match result {
                    Ok(val) => return Ok(Async::Ready(val)),
                    Err(err) => return Err(err)
                }
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
