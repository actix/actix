use std;

use futures::{self, Async, Future, Poll, Stream};
use futures::unsync::mpsc::{unbounded, UnboundedReceiver};
use futures::sync::mpsc::{unbounded as sync_unbounded,
                          UnboundedReceiver as SyncUnboundedReceiver};
use tokio_core::reactor::Handle;

use fut::ActorFuture;
use actor::{Actor, Message};
use address::{Address, SyncAddress, BoxedMessageProxy};
use sink::{Sink, SinkContext, SinkContextService};

macro_rules! try_service {
    ($e:expr) => (match $e {
        $crate::ActorStatus::Done => return Ok($crate::context::Async::Ready(())),
        $crate::ActorStatus::NotReady => (),
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
pub struct Context<A> where A: Actor,
{
    act: A,
    addr: Address<A>,
    sync_addr: Option<SyncAddress<A>>,
    flags: State,
    msgs: UnboundedReceiver<BoxedMessageProxy<A>>,
    sync_msgs: Option<SyncUnboundedReceiver<BoxedMessageProxy<A>>>,
    items: Vec<IoItem<A>>,
    stream: Option<Box<Stream<Item=<A::Message as Message>::Item,
                              Error=<A::Message as Message>::Error>>>,
}

/// io items
enum IoItem<A: Actor> {
    Future(Box<ActFuture<A>>),
    Stream(Box<ActStream<A>>),
    SpawnFuture(Box<ActSpawnFuture<A>>),
    Sink(Box<SinkContextService<A>>),
}

type ActSpawnFuture<A> =
    ActorFuture<Item=(), Error=(), Actor=A>;

type ActFuture<A> =
    Future<Item=<<A as Actor>::Message as Message>::Item,
           Error=<<A as Actor>::Message as Message>::Error>;

type ActStream<A> =
    Stream<Item=<<A as Actor>::Message as Message>::Item,
           Error=<<A as Actor>::Message as Message>::Error>;


impl<A> Context<A> where A: Actor
{
    pub(crate) fn new<S>(act: A, stream: S) -> Context<A>
        where S: Stream<Item=<A::Message as Message>::Item,
                        Error=<A::Message as Message>::Error> + 'static,
    {
        let (tx, rx) = unbounded();

        Context {
            act: act,
            msgs: rx,
            addr: Address::new(tx),
            sync_addr: None,
            sync_msgs: None,
            flags: State::empty(),
            stream: Some(Box::new(stream)),
            items: Vec::new(),
        }
    }

    pub(crate) fn new_empty(act: A) -> Context<A>
    {
        let (tx, rx) = unbounded();
        Context {
            act: act,
            msgs: rx,
            sync_addr: None,
            sync_msgs: None,
            addr: Address::new(tx),
            flags: State::empty(),
            stream: None,
            items: Vec::new(),
        }
    }

    pub(crate) fn run(self, handle: &Handle) -> Address<A> where A: 'static {
        let addr = self.address();
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
        addr
    }

    pub(crate) fn replace_actor(&mut self, srv: A) -> A {
        std::mem::replace(&mut self.act, srv)
    }

    /// Get service address
    pub fn address(&self) -> Address<A> {
        self.addr.clone()
    }

    /// Get service address with `Send` baundary
    pub fn sync_address(&mut self) -> SyncAddress<A> {
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
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.items.push(IoItem::SpawnFuture(Box::new(fut)))
    }

    pub fn add_future<F>(&mut self, fut: F)
        where F: Future<Item=<<A as Actor>::Message as Message>::Item,
                        Error=<<A as Actor>::Message as Message>::Error> + 'static
    {
        self.items.push(IoItem::Future(Box::new(fut)))
    }

    pub fn add_stream<S>(&mut self, fut: S)
        where S: Stream<Item=<<A as Actor>::Message as Message>::Item,
                        Error=<<A as Actor>::Message as Message>::Error> + 'static
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

impl<A> Future for Context<A> where A: Actor
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut Context<A> = unsafe {
            std::mem::transmute(self as &mut Context<A>)
        };
        if !self.flags.contains(State::STARTED) {
            self.flags |= State::STARTED;
            Actor::start(&mut self.act, ctx);
        }

        loop {
            let mut not_ready = true;

            if let Some(ref mut stream) = self.stream {
                match stream.poll() {
                    Ok(val) => {
                        match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                try_service!(Actor::call(&mut self.act, Ok(val), ctx));
                            }
                            Async::Ready(None) =>
                                try_service!(Actor::finished(&mut self.act, ctx)),
                            Async::NotReady => (),
                        }
                    }
                    Err(err) => try_service!(Actor::call(&mut self.act, Err(err), ctx))
                }
            }

            // check messages
            match self.msgs.poll() {
                Ok(val) => {
                    match val {
                        Async::Ready(Some(mut msg)) => {
                            not_ready = false;
                            msg.0.handle(&mut self.act, ctx);
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
                                msg.0.handle(&mut self.act, ctx);
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
                        try_service!(sink.poll(&mut self.act, ctx));
                        (false, None)
                    }
                    IoItem::Stream(ref mut stream) => match stream.poll() {
                        Ok(val) => match val {
                            Async::Ready(Some(val)) => {
                                not_ready = false;
                                try_service!(Actor::call(&mut self.act, Ok(val), ctx));
                                (false, None)
                            }
                            Async::Ready(None) => (true, None),
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            try_service!(Actor::call(&mut self.act, Err(err), ctx));
                            (true, None)
                        }
                    },
                    IoItem::Future(ref mut fut) => match fut.poll() {
                        Ok(val) => match val {
                            Async::Ready(val) => {
                                not_ready = false;
                                try_service!(Actor::call(&mut self.act, Ok(val), ctx));
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        }
                        Err(err) => {
                            try_service!(Actor::call(&mut self.act, Err(err), ctx));
                            (true, None)
                        }
                    }
                    IoItem::SpawnFuture(ref mut fut) => match fut.poll(&mut self.act, ctx) {
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


pub trait FutureSpawner<A> where A: Actor {
    /// spawn future into Context
    fn spawn(self, fut: &mut Context<A>);
}


impl<A, T> FutureSpawner<A> for T
    where A: Actor,
          T: ActorFuture<Item=(), Error=(), Actor=A> + 'static
{
    fn spawn(self, ctx: &mut Context<A>) {
        ctx.spawn(self)
    }
}
