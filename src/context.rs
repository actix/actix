use actor::{Actor, ActorContext, ActorState, AsyncContext, SpawnHandle};
use address::{Addr, AddressReceiver};
use arbiter::Arbiter;
use fut::ActorFuture;

use contextimpl::{AsyncContextParts, ContextFut, ContextParts};
use mailbox::Mailbox;

/// Actor execution context
pub struct Context<A>
where
    A: Actor<Context = Context<A>>,
{
    parts: ContextParts<A>,
    mb: Option<Mailbox<A>>,
}

impl<A> ActorContext for Context<A>
where
    A: Actor<Context = Self>,
{
    #[inline]
    fn stop(&mut self) {
        self.parts.stop()
    }
    #[inline]
    fn terminate(&mut self) {
        self.parts.terminate()
    }
    #[inline]
    fn state(&self) -> ActorState {
        self.parts.state()
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
        self.parts.spawn(fut)
    }

    #[inline]
    fn wait<F>(&mut self, fut: F)
    where
        F: ActorFuture<Item = (), Error = (), Actor = A> + 'static,
    {
        self.parts.wait(fut)
    }

    #[inline]
    fn waiting(&self) -> bool {
        self.parts.waiting()
    }

    #[inline]
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.parts.cancel_future(handle)
    }

    #[inline]
    fn address(&self) -> Addr<A> {
        self.parts.address()
    }
}

impl<A> Context<A>
where
    A: Actor<Context = Self>,
{
    #[inline]
    pub(crate) fn new() -> Context<A> {
        let mb = Mailbox::default();
        Context {
            parts: ContextParts::new(mb.sender_producer()),
            mb: Some(mb),
        }
    }

    #[inline]
    pub fn with_receiver(rx: AddressReceiver<A>) -> Context<A> {
        let mb = Mailbox::new(rx);
        Context {
            parts: ContextParts::new(mb.sender_producer()),
            mb: Some(mb),
        }
    }

    #[inline]
    pub fn run(self, act: A) -> Addr<A> {
        let fut = self.into_future(act);
        let addr = fut.address();
        Arbiter::spawn(fut);
        addr
    }

    pub fn into_future(mut self, act: A) -> ContextFut<A, Self> {
        let mb = self.mb.take().unwrap();
        ContextFut::new(self, act, mb)
    }

    /// Handle of the running future
    ///
    /// SpawnHandle is the handle returned by `AsyncContext::spawn()` method.
    pub fn handle(&self) -> SpawnHandle {
        self.parts.curr_handle()
    }

    /// Set mailbox capacity
    ///
    /// By default mailbox capacity is 16 messages.
    pub fn set_mailbox_capacity(&mut self, cap: usize) {
        self.parts.set_mailbox_capacity(cap)
    }
}

impl<A> AsyncContextParts<A> for Context<A>
where
    A: Actor<Context = Self>,
{
    fn parts(&mut self) -> &mut ContextParts<A> {
        &mut self.parts
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
