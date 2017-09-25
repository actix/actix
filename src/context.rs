use std;

use futures::{self, Async, Future, Poll, Stream};
use futures::unsync::mpsc::{unbounded, UnboundedReceiver};
use futures::sync::mpsc::{unbounded as sync_unbounded,
                          UnboundedReceiver as SyncUnboundedReceiver};
use tokio_core::reactor::Handle;

use service::{Message, Service};

use fut::CtxFuture;
use address::{Address, SyncAddress, BoxedMessageProxy};
use sink::{Sink, SinkContext, SinkContextService};

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

/// Service execution context
pub struct Context<T> where T: Service,
{
    srv: T,
    addr: Address<T>,
    sync_addr: Option<SyncAddress<T>>,
    flags: State,
    msgs: UnboundedReceiver<BoxedMessageProxy<T>>,
    sync_msgs: Option<SyncUnboundedReceiver<BoxedMessageProxy<T>>>,
    items: Vec<IoItem<T>>,
    stream: Option<Box<Stream<Item=<T::Message as Message>::Item,
                              Error=<T::Message as Message>::Error>>>,
}

/// io items
enum IoItem<T: Service> {
    Future(Box<ServiceFuture<T>>),
    Stream(Box<ServiceStream<T>>),
    SpawnFuture(Box<ServiceSpawnFuture<T>>),
    Sink(Box<SinkContextService<T>>),
}

type ServiceSpawnFuture<T> =
    CtxFuture<Item=(), Error=(), Service=T>;

type ServiceFuture<T> =
    Future<Item=<<T as Service>::Message as Message>::Item,
           Error=<<T as Service>::Message as Message>::Error>;

pub type ServiceStream<T> =
    Stream<Item=<<T as Service>::Message as Message>::Item,
           Error=<<T as Service>::Message as Message>::Error>;


impl<T> Context<T> where T: Service
{
    pub(crate) fn new<S>(srv: T, stream: S) -> Context<T>
        where S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static,
    {
        let (tx, rx) = unbounded();

        Context {
            srv: srv,
            msgs: rx,
            addr: Address::new(tx),
            sync_addr: None,
            sync_msgs: None,
            flags: State::empty(),
            stream: Some(Box::new(stream)),
            items: Vec::new(),
        }
    }

    pub(crate) fn new_empty(srv: T) -> Context<T>
    {
        let (tx, rx) = unbounded();
        Context {
            srv: srv,
            msgs: rx,
            sync_addr: None,
            sync_msgs: None,
            addr: Address::new(tx),
            flags: State::empty(),
            stream: None,
            items: Vec::new(),
        }
    }

    pub(crate) fn run(self, handle: &Handle) -> Address<T> where T: 'static {
        let addr = self.address();
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
        addr
    }

    pub(crate) fn replace_service(&mut self, srv: T) -> T {
        std::mem::replace(&mut self.srv, srv)
    }

    /// Get service address
    pub fn address(&self) -> Address<T> {
        self.addr.clone()
    }

    /// Get service address with `Send` baundary
    pub fn sync_address(&mut self) -> SyncAddress<T> {
        if self.sync_addr.is_none() {
            let (tx, rx) = sync_unbounded();
            self.sync_addr = Some(SyncAddress::new(tx));
            self.sync_msgs = Some(rx);
        }

        if let Some(ref addr) = self.sync_addr {
            return addr.clone()
        }
        unreachable!();
    }

    pub fn set_done(&mut self) {
        self.flags |= State::DONE;
    }

    pub fn spawn<F>(&mut self, fut: F)
        where F: CtxFuture<Item=(), Error=(), Service=T> + 'static
    {
        self.items.push(IoItem::SpawnFuture(Box::new(fut)))
    }

    pub fn add_future<F>(&mut self, fut: F)
        where F: Future<Item=<<T as Service>::Message as Message>::Item,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        self.items.push(IoItem::Future(Box::new(fut)))
    }

    pub fn add_stream<S>(&mut self, fut: S)
        where S: Stream<Item=<<T as Service>::Message as Message>::Item,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        self.items.push(IoItem::Stream(Box::new(fut)))
    }

    pub fn add_sink<M, S>(&mut self, sink: S) -> Sink<M>
        where S: futures::Sink<SinkItem=M> + 'static,
              M: Message<Item=(), Error=S::SinkError>,
    {
        let mut srv = Box::new(SinkContext::new(sink));
        let psrv = srv.as_mut() as *mut _;
        self.items.push(IoItem::Sink(srv));

        let sink = Sink::new(psrv);
        sink
    }
}

impl<T> Future for Context<T> where T: Service
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
                                try_service!(Service::call(&mut self.srv, Ok(val), ctx));
                            }
                            Async::Ready(None) =>
                                try_service!(Service::finished(&mut self.srv, ctx)),
                            Async::NotReady => (),
                        }
                    }
                    Err(err) => try_service!(Service::call(&mut self.srv, Err(err), ctx))
                }
            }

            // check messages
            match self.msgs.poll() {
                Ok(val) => {
                    match val {
                        Async::Ready(Some(mut msg)) => {
                            not_ready = false;
                            msg.0.handle(&mut self.srv, ctx);
                        }
                        Async::Ready(None) => (),
                        Async::NotReady => (),
                    }
                }
                Err(_) => (),
            }

            // check remote messages
            if let Some(ref mut msgs) = self.sync_msgs {
                match msgs.poll() {
                    Ok(val) => {
                        match val {
                            Async::Ready(Some(mut msg)) => {
                                not_ready = false;
                                msg.0.handle(&mut self.srv, ctx);
                            }
                            Async::Ready(None) => (),
                            Async::NotReady => (),
                        }
                    }
                    Err(_) => (),
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
                    IoItem::Sink(ref mut sink) => {
                        try_service!(sink.poll(&mut self.srv, ctx));
                        (false, None)
                    }
                    IoItem::Stream(ref mut stream) => match stream.poll() {
                        Ok(val) => match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                try_service!(Service::call(&mut self.srv, Ok(val), ctx));
                                (false, None)
                            }
                            Async::Ready(None) => (true, None),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            try_service!(Service::call(&mut self.srv, Err(err), ctx));
                            (true, None)
                        }
                    },
                    IoItem::Future(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                try_service!(Service::call(&mut self.srv, Ok(val), ctx));
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            try_service!(Service::call(&mut self.srv, Err(err), ctx));
                            (true, None)
                        }
                    }
                    IoItem::SpawnFuture(ref mut fut) => match fut.poll(&mut self.srv, ctx) {
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


pub trait CtxFutureSpawner<S> where S: Service {

    /// spawn future into Context
    fn spawn(self, fut: &mut Context<S>);

}


impl<T, S> CtxFutureSpawner<S> for T
    where S: Service,
          T: CtxFuture<Item=(), Error=(), Service=S> + 'static
{
    fn spawn(self, ctx: &mut Context<S>) {
        ctx.spawn(self)
    }
}
