use std::{mem, fmt};
use futures::{Future, Poll};
use tokio_core::reactor::Handle;

use fut::ActorFuture;
use queue::{sync, unsync};

use actor::{Actor, Supervised,
            ActorState, ActorContext, AsyncContext, SpawnHandle};
use address::{Address, SyncAddress, Subscriber};
use envelope::Envelope;
use contextimpl::ContextImpl;
use contextcells::ContextProtocol;
use handler::{Handler, ResponseType};


pub trait AsyncContextApi<A> where A: Actor, A::Context: AsyncContext<A> {
    fn unsync_sender(&mut self) -> unsync::UnboundedSender<ContextProtocol<A>>;

    fn unsync_address(&mut self) -> Address<A>;

    fn sync_address(&mut self) -> SyncAddress<A>;
}

/// Actor execution context
pub struct Context<A> where A: Actor, A::Context: AsyncContext<A> + AsyncContextApi<A> {
    inner: ContextImpl<A>,
}

impl<A> ActorContext for Context<A> where A: Actor<Context=Self> {
    #[inline]
    fn stop(&mut self) {
        self.inner.stop()
    }
    #[inline]
    fn terminate(&mut self) {
        self.inner.terminate()
    }
    #[inline]
    fn state(&self) -> ActorState {
        self.inner.state()
    }
}

impl<A> AsyncContext<A> for Context<A> where A: Actor<Context=Self> {
    #[inline]
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.inner.spawn(fut)
    }

    #[inline]
    fn wait<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.inner.wait(fut)
    }

    #[doc(hidden)]
    #[inline]
    fn waiting(&self) -> bool {
        self.inner.waiting()
    }

    #[inline]
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.inner.cancel_future(handle)
    }
}

impl<A> AsyncContextApi<A> for Context<A> where A: Actor<Context=Self> {

    #[inline]
    fn unsync_sender(&mut self) -> unsync::UnboundedSender<ContextProtocol<A>> {
        self.inner.unsync_sender()
    }

    #[inline]
    fn unsync_address(&mut self) -> Address<A> {
        self.inner.unsync_address()
    }

    #[inline]
    fn sync_address(&mut self) -> SyncAddress<A> {
        self.inner.sync_address()
    }
}

#[doc(hidden)]
impl<A> Context<A> where A: Actor<Context=Self> {

    #[inline]
    pub fn subscriber<M>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>, M: ResponseType + 'static
    {
        self.inner.subscriber()
    }

    #[inline]
    pub fn sync_subscriber<M>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send {
        self.inner.sync_subscriber()
    }
}

impl<A> Context<A> where A: Actor<Context=Self> {

    #[inline]
    pub(crate) fn new(act: Option<A>) -> Context<A> {
        Context { inner: ContextImpl::new(act) }
    }

    #[inline]
    pub(crate) fn with_receiver(act: Option<A>,
                                rx: sync::UnboundedReceiver<Envelope<A>>) -> Context<A> {
        Context { inner: ContextImpl::with_receiver(act, rx) }
    }

    #[inline]
    pub(crate) fn run(self, handle: &Handle) {
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
    }

    #[inline]
    pub(crate) fn alive(&mut self) -> bool {
        self.inner.alive()
    }

    #[inline]
    pub(crate) fn restarting(&mut self) where A: Supervised {
        let ctx: &mut Context<A> = unsafe {
            mem::transmute(self as &mut Context<A>)
        };
        self.inner.actor().restarting(ctx);
    }

    #[inline]
    pub(crate) fn set_actor(&mut self, act: A) {
        self.inner.set_actor(act)
    }

    #[inline]
    pub(crate) fn into_inner(self) -> A {
        self.inner.into_inner().unwrap()
    }
}

#[doc(hidden)]
impl<A> Future for Context<A> where A: Actor<Context=Self>
{
    type Item = ();
    type Error = ();

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut Context<A> = unsafe {
            mem::transmute(self as &mut Context<A>)
        };
        self.inner.poll(ctx)
    }
}

impl<A> fmt::Debug for Context<A> where A: Actor<Context=Self> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    where A: Actor, A::Context: AsyncContext<A>,
          T: ActorFuture<Item=(), Error=(), Actor=A> + 'static
{
    #[inline]
    fn spawn(self, ctx: &mut A::Context) {
        let _ = ctx.spawn(self);
    }

    #[inline]
    fn wait(self, ctx: &mut A::Context) {
        ctx.wait(self);
    }
}
