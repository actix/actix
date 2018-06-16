use std::fmt;

use futures::{Future, Poll};

use actor::{Actor, ActorContext, ActorState, AsyncContext, SpawnHandle, Supervised};
use address::{Addr, AddressReceiver};
use arbiter::Arbiter;
use contextimpl::ContextImpl;
use fut::ActorFuture;

/// Actor execution context
pub struct Context<A>
where
    A: Actor<Context = Context<A>>,
{
    inner: ContextImpl<A>,
}

impl<A> ActorContext for Context<A>
where
    A: Actor<Context = Self>,
{
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

impl<A> AsyncContext<A> for Context<A>
where
    A: Actor<Context = Self>,
{
    #[inline]
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
    where
        F: ActorFuture<Item = (), Error = (), Actor = A> + 'static,
    {
        self.inner.spawn(fut)
    }

    #[inline]
    fn wait<F>(&mut self, fut: F)
    where
        F: ActorFuture<Item = (), Error = (), Actor = A> + 'static,
    {
        self.inner.wait(fut)
    }

    #[inline]
    fn waiting(&self) -> bool {
        self.inner.waiting()
    }

    #[inline]
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.inner.cancel_future(handle)
    }

    #[inline]
    fn address(&self) -> Addr<A> {
        self.inner.address()
    }
}

impl<A> Context<A>
where
    A: Actor<Context = Self>,
{
    /// Create `Context` instance with actor factory method.
    pub(crate) fn create<F>(f: F) -> Self
    where
        F: FnOnce(&mut Self) -> A + 'static,
    {
        let mut ctx = Context::new(None);
        let act = f(&mut ctx);
        ctx.set_actor(act);
        ctx
    }

    /// Handle of the running future
    ///
    /// SpawnHandle is the handle returned by `AsyncContext::spawn()` method.
    pub fn handle(&self) -> SpawnHandle {
        self.inner.curr_handle()
    }

    /// Set mailbox capacity
    ///
    /// By default mailbox capacity is 16 messages.
    pub fn set_mailbox_capacity(&mut self, cap: usize) {
        self.inner.set_mailbox_capacity(cap)
    }

    #[inline]
    pub(crate) fn new(act: Option<A>) -> Context<A> {
        Context {
            inner: ContextImpl::new(act),
        }
    }

    #[inline]
    pub(crate) fn with_receiver(act: Option<A>, rx: AddressReceiver<A>) -> Context<A> {
        Context {
            inner: ContextImpl::with_receiver(act, rx),
        }
    }

    #[inline]
    pub(crate) fn run(self) {
        Arbiter::spawn(self.map(|_| ()).map_err(|_| ()));
    }

    #[inline]
    pub(crate) fn restart(&mut self) -> bool
    where
        A: Supervised,
    {
        let ctx: &mut Context<A> = unsafe { &mut *(self as *mut _) };
        self.inner.restart(ctx)
    }

    #[inline]
    pub(crate) fn set_actor(&mut self, act: A) {
        self.inner.set_actor(act)
    }
}

#[doc(hidden)]
impl<A> Future for Context<A>
where
    A: Actor<Context = Self>,
{
    type Item = ();
    type Error = ();

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut Context<A> = unsafe { &mut *(self as *mut _) };
        self.inner.poll(ctx)
    }
}

impl<A> fmt::Debug for Context<A>
where
    A: Actor<Context = Self>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Context({:?})", self as *const _)
    }
}

/// Helper trait which can spawn future into actor's context
pub trait ContextFutureSpawner<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    /// spawn future into `Context<A>`
    fn spawn(self, ctx: &mut A::Context);

    /// Spawn future into the context. Stop processing any of incoming events
    /// until this future resolves.
    fn wait(self, ctx: &mut A::Context);
}

impl<A, T> ContextFutureSpawner<A> for T
where
    A: Actor,
    A::Context: AsyncContext<A>,
    T: ActorFuture<Item = (), Error = (), Actor = A> + 'static,
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
