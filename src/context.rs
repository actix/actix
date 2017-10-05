use std;

use futures::{Async, Future, Poll, Stream};
use futures::unsync::oneshot::Sender;
use tokio_core::reactor::Handle;

use fut::ActorFuture;
use queue::{sync, unsync};

use actor::{Actor, Supervised, Handler, StreamHandler,
            ActorState, ActorContext, AsyncActorContext};
use address::{Address, SyncAddress, Subscriber};
use envelope::Envelope;
use message::Response;

pub trait AsyncContextApi<A> where A: Actor, A::Context: AsyncActorContext<A> {
    fn address_cell(&mut self) -> &mut ActorAddressCell<A>;
}

/// context protocol
pub enum ContextProtocol<A: Actor> {
    /// message envelope
    Envelope(Envelope<A>),
    /// Request sync address
    Upgrade(Sender<SyncAddress<A>>),
}

/// Actor execution context
pub struct Context<A> where A: Actor<Context=Context<A>>,
{
    act: A,
    state: ActorState,
    items: Vec<Box<ActorFuture<Item=(), Error=(), Actor=A>>>,
    address: ActorAddressCell<A>,
}

impl<A> ActorContext<A> for Context<A> where A: Actor<Context=Self>
{
    /// Stop actor execution
    fn stop(&mut self) {
        self.address.close();
        if self.state == ActorState::Running {
            self.state = ActorState::Stopping;
        }
    }

    /// Terminate actor execution
    fn terminate(&mut self) {
        self.address.close();
        self.items.clear();
        self.state = ActorState::Stopped;
    }

    /// Actor execution state
    fn state(&self) -> ActorState {
        self.state
    }
}

impl<A> AsyncActorContext<A> for Context<A> where A: Actor<Context=Self>
{
    fn spawn<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        if self.state == ActorState::Stopped {
            error!("Context::spawn called for stopped actor.");
        } else {
            self.items.push(Box::new(fut))
        }
    }
}

impl<A> AsyncContextApi<A> for Context<A> where A: Actor<Context=Self> {
    fn address_cell(&mut self) -> &mut ActorAddressCell<A> {
        &mut self.address
    }
}

impl<A> Context<A> where A: Actor<Context=Self>
{
    #[doc(hidden)]
    pub fn subscriber<M: 'static>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>
    {
        Box::new(self.address.unsync_address())
    }

    #[doc(hidden)]
    pub fn sync_subscriber<M: 'static + Send>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              A::Item: Send,
              A::Error: Send,
    {
        self.address::<SyncAddress<_>>().subscriber()
    }
}

impl<A> Context<A> where A: Actor<Context=Self>
{
    pub(crate) fn new(act: A) -> Context<A>
    {
        Context {
            act: act,
            state: ActorState::Started,
            items: Vec::new(),
            address: ActorAddressCell::new(),
        }
    }

    pub(crate) fn run(self, handle: &Handle) {
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
    }

    pub(crate) fn alive(&mut self) -> bool {
        if self.state == ActorState::Stopped {
            false
        } else {
            self.address.connected() || !self.items.is_empty()
        }
    }

    pub(crate) fn restarting(&mut self) where A: Supervised {
        let ctx: &mut Context<A> = unsafe {
            std::mem::transmute(self as &mut Context<A>)
        };
        self.act.restarting(ctx);
    }

    pub(crate) fn replace_actor(&mut self, srv: A) -> A {
        std::mem::replace(&mut self.act, srv)
    }

    pub(crate) fn into_inner(self) -> A {
        self.act
    }
}

#[doc(hidden)]
impl<A> Future for Context<A> where A: Actor<Context=Self>
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut Context<A> = unsafe {
            std::mem::transmute(self as &mut Context<A>)
        };

        // update state
        match self.state {
            ActorState::Started => {
                Actor::started(&mut self.act, ctx);
                self.state = ActorState::Running;
            },
            ActorState::Stopping => {
                Actor::stopping(&mut self.act, ctx);
            }
            _ => ()
        }

        let mut prep_stop = false;

        loop {
            let mut not_ready = true;

            if let Ok(Async::Ready(_)) = self.address.poll(&mut self.act, ctx) {
                not_ready = false
            }

            // check secondary streams
            let mut idx = 0;
            let mut len = self.items.len();
            loop {
                if idx >= len {
                    break
                }

                let (drop, item) = match self.items[idx].poll(&mut self.act, ctx) {
                    Ok(val) => match val {
                        Async::Ready(_) => {
                            not_ready = false;
                            (true, None)
                        }
                        Async::NotReady => (false, None),
                    },
                    Err(_) => (true, None)
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

            // are we done
            if !not_ready {
                continue
            }

            // check state
            match self.state {
                ActorState::Stopped => {
                    self.state = ActorState::Stopped;
                    Actor::stopped(&mut self.act, ctx);
                    return Ok(Async::Ready(()))
                },
                ActorState::Stopping => {
                    if prep_stop {
                        if self.address.connected() || !self.items.is_empty() {
                            self.state = ActorState::Running;
                            continue
                        } else {
                            self.state = ActorState::Stopped;
                            Actor::stopped(&mut self.act, ctx);
                            return Ok(Async::Ready(()))
                        }
                    } else {
                        Actor::stopping(&mut self.act, ctx);
                        prep_stop = true;
                        continue
                    }
                },
                ActorState::Running => {
                    if !self.address.connected() && self.items.is_empty() {
                        self.state = ActorState::Stopping;
                        Actor::stopping(&mut self.act, ctx);
                        prep_stop = true;
                        continue
                    }
                },
                _ => (),
            }

            return Ok(Async::NotReady)
        }
    }
}


pub struct ActorAddressCell<A> where A: Actor, A::Context: AsyncActorContext<A>
{
    sync_alive: bool,
    sync_msgs: Option<sync::UnboundedReceiver<Envelope<A>>>,
    unsync_msgs: unsync::UnboundedReceiver<ContextProtocol<A>>,
}

impl<A> ActorAddressCell<A> where A: Actor, A::Context: AsyncActorContext<A>
{
    pub fn new() -> ActorAddressCell<A> {
        ActorAddressCell {
            sync_alive: false,
            sync_msgs: None,
            unsync_msgs: unsync::unbounded(),
        }
    }

    pub fn close(&mut self) {
        self.unsync_msgs.close();
        if let Some(ref mut msgs) = self.sync_msgs {
            msgs.close()
        }
    }

    pub fn connected(&mut self) -> bool {
        self.unsync_msgs.connected() || self.sync_alive
    }

    pub fn unsync_sender(&mut self) -> unsync::UnboundedSender<ContextProtocol<A>> {
        self.unsync_msgs.sender()
    }

    pub fn unsync_address(&mut self) -> Address<A> {
        Address::new(self.unsync_msgs.sender())
    }

    pub fn sync_address(&mut self) -> SyncAddress<A> {
        if self.sync_msgs.is_none() {
            let (tx, rx) = sync::unbounded();
            self.sync_msgs = Some(rx);
            self.sync_alive = true;
            SyncAddress::new(tx)
        } else {
            if let Some(ref mut addr) = self.sync_msgs {
                return SyncAddress::new(addr.sender())
            }
            unreachable!();
        }
    }
}

impl<A> ActorFuture for ActorAddressCell<A> where A: Actor, A::Context: AsyncActorContext<A>
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error>
    {
        loop {
            let mut not_ready = true;

            // unsync messages
            match self.unsync_msgs.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    not_ready = false;
                    match msg {
                        ContextProtocol::Envelope(mut env) => {
                            env.handle(act, ctx)
                        }
                        ContextProtocol::Upgrade(tx) => {
                            let _ = tx.send(self.sync_address());
                        }
                    }
                }
                Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => (),
            }

            // sync messages
            if self.sync_alive {
                if let Some(ref mut msgs) = self.sync_msgs {
                    match msgs.poll() {
                        Ok(Async::Ready(Some(mut msg))) => {
                            not_ready = false;
                            msg.handle(act, ctx);
                        }
                        Ok(Async::Ready(None)) | Err(_) => {
                            self.sync_alive = false;
                        },
                        Ok(Async::NotReady) => (),
                    }
                }
            }

            if not_ready {
                return Ok(Async::NotReady)
            }
        }
    }
}


pub(crate)
struct ActorFutureCell<A, M, F, E>
    where A: Actor + Handler<M, E>,
          A::Context: AsyncActorContext<A>,
          F: Future<Item=M, Error=E>,
{
    act: std::marker::PhantomData<A>,
    fut: F,
    result: Option<Response<A, M>>,
}

impl<A, M, F, E> ActorFutureCell<A, M, F, E>
    where A: Actor + Handler<M, E>,
          A::Context: AsyncActorContext<A>,
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
    where A: Actor + Handler<M, E>,
          A::Context: AsyncActorContext<A>,
          F: Future<Item=M, Error=E>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error>
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
                    let fut = <Self::Actor as Handler<M, E>>::handle(act, msg, ctx);
                    self.result = Some(fut);
                    continue
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(err) => {
                    <Self::Actor as Handler<M, E>>::error(act, err, ctx);
                    return Err(())
                }
            }
        }
    }
}

pub(crate)
struct ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + Handler<M, E> + StreamHandler<M, E>,
          A::Context: AsyncActorContext<A>
{
    act: std::marker::PhantomData<A>,
    started: bool,
    fut: Option<Response<A, M>>,
    stream: S,
}

impl<A, M, E, S> ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E> + 'static,
          A: Actor + Handler<M, E> + StreamHandler<M, E>,
          A::Context: AsyncActorContext<A>
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
          A: Actor + Handler<M, E> + StreamHandler<M, E>,
          A::Context: AsyncActorContext<A>
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error>
    {
        if !self.started {
            self.started = true;
            <A as StreamHandler<M, E>>::started(act, ctx);
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
                    let fut = <Self::Actor as Handler<M, E>>::handle(act, msg, ctx);
                    self.fut = Some(fut);
                    continue
                }
                Ok(Async::Ready(None)) => {
                    <A as StreamHandler<M, E>>::finished(act, ctx);
                    return Ok(Async::Ready(()))
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(err) => {
                    <Self::Actor as Handler<M, E>>::error(act, err, ctx);
                    return Err(())
                }
            }
        }
    }
}

/// Helper trait which can spawn future into actor's context
pub trait ContextFutureSpawner<A> where A: Actor, A::Context: AsyncActorContext<A> {
    /// spawn future into `Context<A>`
    fn spawn(self, fut: &mut A::Context);
}


impl<A, T> ContextFutureSpawner<A> for T
    where A: Actor,
          A::Context: AsyncActorContext<A>,
          T: ActorFuture<Item=(), Error=(), Actor=A> + 'static
{
    fn spawn(self, ctx: &mut A::Context) {
        ctx.spawn(self)
    }
}
