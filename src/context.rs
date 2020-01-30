use std::fmt;

use crate::actor::{Actor, ActorContext, ActorState, AsyncContext, SpawnHandle};
use crate::address::{Addr, AddressReceiver};
use crate::contextimpl::{AsyncContextParts, ContextFut, ContextParts};
use crate::fut::ActorFuture;
use crate::mailbox::Mailbox;

/// An actor execution context.
pub struct Context<A>
where
    A: Actor<Context = Context<A>>,
{
    parts: ContextParts<A>,
    mb: Option<Mailbox<A>>,
}

impl<A: Actor<Context = Context<A>>> fmt::Debug for Context<A> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Context")
            .field("parts", &self.parts)
            .field("mb", &self.mb)
            .finish()
    }
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
        F: ActorFuture<Output = (), Actor = A> + 'static,
    {
        self.parts.spawn(fut)
    }

    #[inline]
    fn wait<F>(&mut self, fut: F)
    where
        F: ActorFuture<Output = (), Actor = A> + 'static,
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
    pub(crate) fn new() -> Self {
        let mb = Mailbox::default();
        Self {
            parts: ContextParts::new(mb.sender_producer()),
            mb: Some(mb),
        }
    }

    #[inline]
    pub fn with_receiver(rx: AddressReceiver<A>) -> Self {
        let mb = Mailbox::new(rx);
        Self {
            parts: ContextParts::new(mb.sender_producer()),
            mb: Some(mb),
        }
    }

    #[inline]
    pub fn run(self, act: A) -> Addr<A> {
        let fut = self.into_future(act);
        let addr = fut.address();
        actix_rt::spawn(fut);
        addr
    }

    pub fn into_future(mut self, act: A) -> ContextFut<A, Self> {
        let mb = self.mb.take().unwrap();
        ContextFut::new(self, act, mb)
    }

    /// Returns a handle to the running future.
    ///
    /// This is the handle returned by the `AsyncContext::spawn()`
    /// method.
    pub fn handle(&self) -> SpawnHandle {
        self.parts.curr_handle()
    }

    /// Sets the mailbox capacity.
    ///
    /// The default mailbox capacity is 16 messages.
    /// #Examples
    /// ```
    /// # use actix::prelude::*;
    /// struct MyActor;
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    ///
    ///     fn started(&mut self, ctx: &mut Self::Context) {
    ///         ctx.set_mailbox_capacity(1);
    ///     }
    /// }
    ///
    /// # fn main() {
    /// # System::new("test");
    /// let addr = MyActor.start();
    /// # }
    /// ```
    pub fn set_mailbox_capacity(&mut self, cap: usize) {
        self.parts.set_mailbox_capacity(cap)
    }

    /// Returns whether any addresses are still connected.
    pub fn connected(&self) -> bool {
        self.parts.connected()
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

/// Helper trait which can spawn a future into the actor's context.
pub trait ContextFutureSpawner<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    /// Spawns the future into the given context.
    fn spawn(self, ctx: &mut A::Context);

    /// Spawns the future into the given context, waiting for it to
    /// resolve.
    ///
    /// This stops processing any incoming events until this future
    /// resolves.
    fn wait(self, ctx: &mut A::Context);
}

impl<A, T> ContextFutureSpawner<A> for T
where
    A: Actor,
    A::Context: AsyncContext<A>,
    T: ActorFuture<Output = (), Actor = A> + 'static,
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
