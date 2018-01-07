use std;
use std::collections::{VecDeque, HashSet};

use futures::{Async, Future, Poll, Stream};
use futures::unsync::oneshot::Sender;
use tokio_core::reactor::Handle;

use fut::ActorFuture;
use queue::{sync, unsync};

use actor::{Actor, Supervised,
            ActorState, ActorContext, AsyncContext, SpawnHandle};
use address::{Address, SyncAddress, Subscriber};
use envelope::Envelope;
use message::Response;
use constants::MAX_SYNC_POLLS;
use handler::{Handler, ResponseType, IntoResponse};
use contextimpl::ContextImpl;
use contextcells::{ActorAddressCell, ActorItemsCell, ActorWaitCell, ContextCellResult};

pub trait AsyncContextApi<A> where A: Actor, A::Context: AsyncContext<A> {
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
pub struct Context<A> where A: Actor, A::Context: AsyncContext<A> {
    inner: ContextImpl<A, ()>,
}

impl<A> ActorContext for Context<A> where A: Actor<Context=Self>
{
    /// Stop actor execution
    fn stop(&mut self) {
        self.inner.stop()
    }

    /// Terminate actor execution
    fn terminate(&mut self) {
        self.inner.terminate()
    }

    /// Actor execution state
    fn state(&self) -> ActorState {
        self.inner.state()
    }
}

impl<A> AsyncContext<A> for Context<A> where A: Actor<Context=Self>
{
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.inner.spawn(fut)
    }

    fn wait<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.inner.wait(fut)
    }

    fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.inner.cancel_future(handle)
    }
}

impl<A> AsyncContextApi<A> for Context<A> where A: Actor<Context=Self> {
    fn address_cell(&mut self) -> &mut ActorAddressCell<A> {
        self.inner.address_cell()
    }
}

impl<A> Context<A> where A: Actor<Context=Self>
{
    #[doc(hidden)]
    pub fn subscriber<M>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>,
              M: ResponseType + 'static {
        self.inner.subscriber()
    }

    #[doc(hidden)]
    pub fn sync_subscriber<M>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              M: ResponseType + Send + 'static,
              M::Item: Send,
              M::Error: Send,
    {
        self.inner.sync_subscriber()
    }
}

impl<A> Context<A> where A: Actor<Context=Self>
{
    pub(crate) fn new(act: Option<A>) -> Context<A> {
        Context { inner: ContextImpl::<_, ()>::new(act) }
    }

    pub(crate) fn with_receiver(act: Option<A>,
                                rx: sync::UnboundedReceiver<Envelope<A>>) -> Context<A> {
        Context { inner: ContextImpl::<_, ()>::with_receiver(act, rx) }
    }

    pub(crate) fn run(self, handle: &Handle) {
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
    }

    pub(crate) fn alive(&mut self) -> bool {
        self.inner.alive()
    }

    pub(crate) fn restarting(&mut self) where A: Supervised {
        let ctx: &mut Context<A> = unsafe {
            std::mem::transmute(self as &mut Context<A>)
        };
        self.inner.actor().restarting(ctx);
    }

    pub(crate) fn set_actor(&mut self, act: A) {
        self.inner.set_actor(act)
    }

    pub(crate) fn into_inner(self) -> A {
        self.inner.into_inner().unwrap()
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

        self.inner.poll(ctx)
    }
}

impl<A> std::fmt::Debug for Context<A> where A: Actor<Context=Self> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Context({:?})", self as *const _)
    }
}

/// Helper trait which can spawn future into actor's context
pub trait ContextFutureSpawner<A> where A: Actor, A::Context: AsyncContext<A> {
    /// spawn future into `Context<A>`
    fn spawn(self, ctx: &mut A::Context);

    /// Spawn future into the context. Stop processing any of incoming events
    /// until this future resolves.
    fn wait(self, ctx: &mut A::Context);
}


impl<A, T> ContextFutureSpawner<A> for T
    where A: Actor,
          A::Context: AsyncContext<A>,
          T: ActorFuture<Item=(), Error=(), Actor=A> + 'static
{
    fn spawn(self, ctx: &mut A::Context) {
        let _ = ctx.spawn(self);
    }

    fn wait(self, ctx: &mut A::Context) {
        ctx.wait(self);
    }
}
