use std;

use futures::{self, Async, Future, Poll, Stream};
use futures::unsync::mpsc::{unbounded, UnboundedReceiver};
use futures::sync::mpsc::{unbounded as sync_unbounded,
                          UnboundedReceiver as SyncUnboundedReceiver};
use tokio_core::reactor::Handle;

use fut::ActorFuture;
use actor::{Actor, MessageHandler, StreamHandler};
use address::{ActorAddress, Address, SyncAddress, Proxy, Subscriber};
use message::MessageFuture;
use sink::{Sink, SinkContext, SinkContextService};

bitflags! {
    /// State Bitflags
    struct State: u16 {
        /// Service is started
        const STARTED = 0b000_000_001;
        /// Service is done
        const DONE = 0b000_000_010;
    }
}

/// Actor execution context
pub struct Context<A> where A: Actor,
{
    act: A,
    addr: Address<A>,
    sync_addr: Option<SyncAddress<A>>,
    flags: State,
    msgs: UnboundedReceiver<Proxy<A>>,
    sync_msgs: Option<SyncUnboundedReceiver<Proxy<A>>>,
    items: Vec<IoItem<A>>,
}

/// io items
enum IoItem<A: Actor> {
    SpawnFuture(Box<ActSpawnFuture<A>>),
    Sink(Box<SinkContextService<A>>),
}

type ActSpawnFuture<A> =
    ActorFuture<Item=(), Error=(), Actor=A>;


impl<A> Context<A> where A: Actor
{
    /// Stop actor execution.
    pub fn stop(&mut self) {
        self.flags |= DONE;
    }

    /// Get service address
    pub fn address<Address>(&mut self) -> Address
        where A: ActorAddress<A, Address>
    {
        <A as ActorAddress<A, Address>>::get(self)
    }

    pub fn subscriber<M: 'static>(&self) -> Box<Subscriber<M>>
        where A: MessageHandler<M>
    {
        Box::new(self.addr.clone())
    }

    pub fn sync_subscriber<M: 'static + Send>(&mut self) -> Box<Subscriber<M> + Send>
        where A: MessageHandler<M>,
              A::Item: Send,
              A::Error: Send,
    {
        self.address::<SyncAddress<_>>().subscriber()
    }

    pub fn spawn<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.items.push(IoItem::SpawnFuture(Box::new(fut)))
    }

    pub fn add_future<F, E: 'static>(&mut self, fut: F)
        where F: Future<Error=E> + 'static,
              A: MessageHandler<F::Item, InputError=E>,
    {
        self.spawn(ActorFutureCell::new(fut))
    }

    pub fn add_stream<S, E: 'static>(&mut self, fut: S)
        where S: Stream<Error=E> + 'static,
              A: MessageHandler<S::Item, InputError=E> + StreamHandler<S::Item, InputError=E>,
    {
        self.spawn(ActorStreamCell::new(fut))
    }

    pub fn add_sink<S>(&mut self, sink: S) -> Sink<S::SinkItem, S::SinkError>
        where S: futures::Sink + 'static,
    {
        let mut srv = Box::new(SinkContext::new(sink));
        let psrv = srv.as_mut() as *mut _;
        self.items.push(IoItem::Sink(srv));

        Sink::new(psrv)
    }
}


impl<A> Context<A> where A: Actor
{
    pub(crate) fn new(act: A) -> Context<A>
    {
        let (tx, rx) = unbounded();
        Context {
            act: act,
            msgs: rx,
            sync_addr: None,
            sync_msgs: None,
            addr: Address::new(tx),
            flags: State::empty(),
            items: Vec::new(),
        }
    }

    pub(crate) fn run(self, handle: &Handle) {
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
    }

    pub(crate) fn replace_actor(&mut self, srv: A) -> A {
        std::mem::replace(&mut self.act, srv)
    }

    /// Get service address without `Send` baundary
    pub(crate) fn loc_address(&mut self) -> Address<A> {
        self.addr.clone()
    }

    /// Get service address with `Send` baundary
    pub(crate) fn sync_address(&mut self) -> SyncAddress<A> {
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
}

impl<A> Future for Context<A> where A: Actor
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut Context<A> = unsafe {
            std::mem::transmute(self as &mut Context<A>)
        };
        if !self.flags.contains(STARTED) {
            self.flags |= STARTED;
            Actor::started(&mut self.act, ctx);
        }

        loop {
            let mut not_ready = true;

            // check messages
            match self.msgs.poll() {
                Ok(Async::Ready(Some(mut msg))) => {
                    not_ready = false;
                    msg.0.handle(&mut self.act, ctx);
                }
                Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => (),
            }

            // check remote messages
            if let Some(ref mut msgs) = self.sync_msgs {
                match msgs.poll() {
                    Ok(Async::Ready(Some(mut msg))) => {
                        not_ready = false;
                        msg.0.handle(&mut self.act, ctx);
                    }
                    Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => (),
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
                    IoItem::Sink(ref mut sink) => match sink.poll(&mut self.act, ctx) {
                        Async::Ready(_) => (true, None),
                        Async::NotReady => (false, None)
                    },
                    IoItem::SpawnFuture(ref mut fut) => match fut.poll(&mut self.act, ctx) {
                        Ok(val) => match val {
                            Async::Ready(_) => {
                                not_ready = false;
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        },
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
                    len -= 1;
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

            if self.flags.contains(DONE) {
                Actor::finished(&mut self.act, ctx);
                return Ok(Async::Ready(()))
            }

            // are we done
            if not_ready {
                return Ok(Async::NotReady)
            }
        }
    }
}


struct ActorFutureCell<A, M, F, E>
    where A: Actor + MessageHandler<M, InputError=E>,
          F: Future<Item=M, Error=E>,
{
    act: std::marker::PhantomData<A>,
    fut: F,
    result: Option<MessageFuture<A, M>>,
}

impl<A, M, F, E> ActorFutureCell<A, M, F, E>
    where A: Actor + MessageHandler<M, InputError=E>,
          F: Future<Item=M, Error=E>,
{
    pub fn new(fut: F) -> ActorFutureCell<A, M, F, E>
    {
        ActorFutureCell {
            act: std::marker::PhantomData,
            fut: fut,
            result: None }
    }
}

impl<A, M, F, E> ActorFuture for ActorFutureCell<A, M, F, E>
    where A: Actor + MessageHandler<M, InputError=E>,
          F: Future<Item=M, Error=E>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut Context<A>) -> Poll<Self::Item, Self::Error>
    {
        loop {
            if let Some(mut fut) = self.result.take() {
                match fut.poll(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.result = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) =>
                        return Ok(Async::Ready(())),
                    Err(_) =>
                        return Err(())
                }
            }

            match self.fut.poll() {
                Ok(Async::Ready(msg)) => {
                    let fut = <Self::Actor as MessageHandler<M>>::handle(act, msg, ctx);
                    self.result = Some(fut);
                    continue
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(err) => {
                    <Self::Actor as MessageHandler<M>>::error(act, err, ctx);
                    return Err(())
                }
            }
        }
    }
}


struct ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + MessageHandler<M, InputError=E> + StreamHandler<M, InputError=E>,
{
    act: std::marker::PhantomData<A>,
    started: bool,
    fut: Option<MessageFuture<A, M>>,
    stream: S,
}

impl<A, M, E, S> ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E> + 'static,
          A: Actor + MessageHandler<M, InputError=E> + StreamHandler<M, InputError=E>,
{
    pub fn new(fut: S) -> ActorStreamCell<A, M, E, S>
    {
        ActorStreamCell {
            act: std::marker::PhantomData,
            started: false,
            fut: None,
            stream: fut }
    }
}

impl<A, M, E, S> ActorFuture for ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + MessageHandler<M, InputError=E> + StreamHandler<M, InputError=E>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut Context<A>) -> Poll<Self::Item, Self::Error>
    {
        if !self.started {
            self.started = true;
            <A as StreamHandler<M>>::started(act, ctx);
        }

        loop {
            if let Some(mut fut) = self.fut.take() {
                match fut.poll(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.fut = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) => (),
                    Err(_) => return Err(())
                }
            }

            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    let fut = <Self::Actor as MessageHandler<M>>::handle(act, msg, ctx);
                    self.fut = Some(fut);
                    continue
                }
                Ok(Async::Ready(None)) => {
                    <A as StreamHandler<M>>::finished(act, ctx);
                    return Ok(Async::Ready(()))
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(err) => {
                    <Self::Actor as MessageHandler<M>>::error(act, err, ctx);
                    return Err(())
                }
            }
        }
    }
}

/// Helper trait which can spawn future into actor's context
pub trait ActixFutureSpawner<A> where A: Actor {
    /// spawn future into `Context<A>`
    fn spawn(self, fut: &mut Context<A>);
}


impl<A, T> ActixFutureSpawner<A> for T
    where A: Actor,
          T: ActorFuture<Item=(), Error=(), Actor=A> + 'static
{
    fn spawn(self, ctx: &mut Context<A>) {
        ctx.spawn(self)
    }
}
