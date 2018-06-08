use std::cell::UnsafeCell;
use std::fmt;
use std::sync::Arc;

use futures::{Future, Poll};
use tokio;

use actor::{
    Actor, ActorContext, ActorState, AsyncContext, Ctx, SpawnHandle, Supervised,
};
use address::{Addr, AddressReceiver};
use contextimpl::ContextImpl;
use fut::ActorFuture;

/// Actor execution context
pub struct Context<A>
where
    A: Actor<Context = Context<A>>,
{
    inner: Arc<UnsafeCell<ContextImpl<A>>>,
}

unsafe impl<A: Actor<Context = Context<A>>> Send for Context<A> {}

impl<A> ActorContext for Context<A>
where
    A: Actor<Context = Self>,
{
    #[inline]
    fn stop(&mut self) {
        self.get_mut().stop()
    }
    #[inline]
    fn terminate(&mut self) {
        self.get_mut().terminate()
    }
    #[inline]
    fn state(&self) -> ActorState {
        self.get_ref().state()
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
        self.get_mut().spawn(fut)
    }

    #[inline]
    fn spawn_and_wait<F>(&mut self, fut: F)
    where
        F: ActorFuture<Item = (), Error = (), Actor = A> + 'static,
    {
        self.get_mut().wait(fut)
    }

    #[inline]
    fn waiting(&self) -> bool {
        self.get_ref().waiting()
    }

    #[inline]
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.get_mut().cancel_future(handle)
    }

    #[inline]
    fn address(&self) -> Addr<A> {
        self.get_ref().address()
    }
}

impl<A> Context<A>
where
    A: Actor<Context = Self>,
{
    /// Create `Context` instance with actor factory method.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use actix::*;
    ///
    /// // initialize system
    /// System::new("test");
    ///
    /// struct MyActor {
    ///     val: usize,
    /// };
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    /// }
    ///
    /// let ctx = Context::create(|ctx: Ctx<MyActor>| MyActor { val: 10 });
    /// ```
    pub fn create<F>(f: F) -> Self
    where
        F: FnOnce(Ctx<A>) -> A + 'static,
    {
        let mut ctx = Context::new(None);
        let link = Ctx::new(Context {
            inner: ctx.inner.clone(),
        });
        let act = f(link);
        ctx.set_actor(act);
        ctx
    }

    /// Handle of the running future
    ///
    /// SpawnHandle is the handle returned by `AsyncContext::spawn()` method.
    pub fn handle(&self) -> SpawnHandle {
        self.get_ref().curr_handle()
    }

    /// Set mailbox capacity
    ///
    /// By default mailbox capacity is 16 messages.
    pub fn set_mailbox_capacity(&mut self, cap: usize) {
        self.get_mut().set_mailbox_capacity(cap)
    }

    #[inline]
    pub(crate) fn new(act: Option<A>) -> Context<A> {
        Context {
            inner: Arc::new(UnsafeCell::new(ContextImpl::new(act))),
        }
    }

    #[inline]
    pub(crate) fn with_receiver(act: Option<A>, rx: AddressReceiver<A>) -> Context<A> {
        Context {
            inner: Arc::new(UnsafeCell::new(ContextImpl::with_receiver(act, rx))),
        }
    }

    #[inline]
    pub(crate) fn run(self)
    where
        A: Send,
    {
        tokio::spawn(self.map(|_| ()).map_err(|_| ()));
    }

    #[inline]
    pub(crate) fn restart(&mut self) -> bool
    where
        A: Supervised,
    {
        let ctx: &mut Context<A> = unsafe { &mut *(self as *mut _) };
        self.get_mut().restart(ctx)
    }

    #[inline]
    pub(crate) fn set_actor(&mut self, act: A) {
        self.get_mut().set_actor(act)
    }

    #[inline]
    fn get_ref(&self) -> &ContextImpl<A> {
        unsafe { &*self.inner.get() }
    }

    #[inline]
    fn get_mut(&mut self) -> &mut ContextImpl<A> {
        unsafe { &mut *self.inner.get() }
    }

    pub(crate) fn clone(&self) -> Self {
        Context {
            inner: self.inner.clone(),
        }
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
        self.get_mut().poll(ctx)
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
    fn spawn_and_wait(self, ctx: &mut A::Context);
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
    fn spawn_and_wait(self, ctx: &mut A::Context) {
        ctx.spawn_and_wait(self);
    }
}
