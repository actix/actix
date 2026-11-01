//! Sync Actors support
//!
//! Sync Actors are actors that run multiple instances on a thread pool.
//! This is useful for CPU bound, or concurrent workloads. There can only be
//! a single Sync Actor type on a `SyncArbiter`. This means you can't have
//! Actor type A and B, sharing the same thread pool. You need to create two
//! [`SyncArbiter`]s and have A and B spawn on unique `SyncArbiter`s respectively.
//! For more information and examples, see `SyncArbiter`
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task,
    task::Poll,
    thread,
};

use actix_rt::System;
use crossbeam_channel as cb_channel;
use futures_core::stream::Stream;
use log::warn;
use tokio::sync::{oneshot::Sender as SyncSender, Notify};

use crate::{
    actor::{Actor, ActorContext, ActorState, Running},
    address::{
        channel, Addr, AddressReceiver, AddressSenderProducer, Envelope, EnvelopeProxy, ToEnvelope,
    },
    context::Context,
    handler::{Handler, Message, MessageResponse},
};

/// [`SyncArbiter`] provides the resources for a single Sync Actor to run on a dedicated
/// thread or threads.
///
/// This is generally used for CPU bound concurrent workloads. It's important to note, that the
/// [`SyncArbiter`] generates a single address for the pool of hosted Sync Actors. Any message sent
/// to this Address, will be operated on by a single Sync Actor from the pool.
///
/// Sync Actors have a different lifecycle compared to Actors on the System Arbiter. For more, see
/// [`SyncContext`].
///
/// [`System::stop`] does not wait for worker threads; use
/// [`SyncArbiter::start_with_handle`] and await [`SyncArbiterHandle::wait`]
/// before stopping the system if queued work must complete.
///
/// # Examples
///
/// ```
/// use actix::prelude::*;
///
/// struct Fibonacci(pub u32);
///
/// # impl Message for Fibonacci {
/// #     type Result = Result<u64, ()>;
/// # }
///
/// struct SyncActor;
///
/// impl Actor for SyncActor {
///     // It's important to note that you use "SyncContext" here instead of "Context".
///     type Context = SyncContext<Self>;
/// }
///
/// impl Handler<Fibonacci> for SyncActor {
///     type Result = Result<u64, ()>;
///
///     fn handle(&mut self, msg: Fibonacci, _: &mut Self::Context) -> Self::Result {
///         if msg.0 == 0 {
///             Err(())
///         } else if msg.0 == 1 {
///             Ok(1)
///         } else {
///             let mut i = 0;
///             let mut sum = 0;
///             let mut last = 0;
///             let mut curr = 1;
///             while i < msg.0 - 1 {
///                 sum = last + curr;
///                 last = curr;
///                 curr = sum;
///                 i += 1;
///             }
///             Ok(sum)
///         }
///     }
/// }
///
/// fn main() {
///     System::new().block_on(async {
///         // Start the SyncArbiter with 2 threads, and receive the address of the Actor pool.
///         let addr = SyncArbiter::start(2, || SyncActor);
///
///         // send 5 messages
///         for n in 5..10 {
///             // As there are 2 threads, there are at least 2 messages always being processed
///             // concurrently by the SyncActor.
///             addr.do_send(Fibonacci(n));
///         }
///
/// #       System::current().stop();
///     });
/// }
/// ```
pub struct SyncArbiter<A>
where
    A: Actor<Context = SyncContext<A>>,
{
    queue: Option<cb_channel::Sender<Envelope<A>>>,
    msgs: AddressReceiver<A>,
}

/// Handle to a running [`SyncArbiter`] whose worker threads can be awaited.
///
/// Obtain one from [`SyncArbiter::start_with_handle`] or
/// [`SyncArbiter::start_once_with_handle`]. Await [`wait`](Self::wait) before
/// stopping the [`System`] to make sure queued messages have been handled.
#[derive(Clone)]
pub struct SyncArbiterHandle {
    state: Arc<WorkerState>,
}

struct WorkerState {
    alive: AtomicUsize,
    finished: Notify,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            alive: AtomicUsize::new(0),
            finished: Notify::new(),
        }
    }

    fn add_worker(&self) {
        self.alive.fetch_add(1, Ordering::SeqCst);
    }

    fn finish_worker(&self) {
        if self.alive.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.finished.notify_waiters();
        }
    }
}

struct WorkerGuard(Arc<WorkerState>);

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.finish_worker();
    }
}

impl SyncArbiterHandle {
    /// Waits until every worker thread of the arbiter has finished.
    ///
    /// Workers exit once all addresses to the arbiter have been dropped and
    /// every queued message has been handled. Await this before stopping the
    /// [`System`] to drain a sync actor, such as a logging worker:
    ///
    /// ```no_run
    /// use actix::prelude::*;
    ///
    /// struct Worker;
    ///
    /// impl Actor for Worker {
    ///     type Context = SyncContext<Self>;
    /// }
    ///
    /// #[actix::main]
    /// async fn main() {
    ///     let (addr, handle) = SyncArbiter::start_with_handle(1, || Worker);
    ///
    ///     // ... send messages through addr ...
    ///
    ///     drop(addr);
    ///     handle.wait().await;
    ///     System::current().stop();
    /// }
    /// ```
    pub async fn wait(&self) {
        loop {
            let notified = self.state.finished.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if self.state.alive.load(Ordering::SeqCst) == 0 {
                return;
            }

            notified.await;
        }
    }
}

impl<A> SyncArbiter<A>
where
    A: Actor<Context = SyncContext<A>>,
{
    /// Start a new `SyncArbiter` with specified number of worker threads.
    /// Returns a single address of the started actor. A single address is
    /// used to communicate to the actor(s), and messages are handled by
    /// the next available Actor in the `SyncArbiter`.
    pub fn start<F>(threads: usize, factory: F) -> Addr<A>
    where
        F: Fn() -> A + Send + Sync + 'static,
    {
        Self::start_with_thread_builder(threads, thread::Builder::new, factory)
    }

    /// Start a new `SyncArbiter` with specified number of worker threads and a
    /// handle that can be used to wait for all workers to finish.
    ///
    /// See [`SyncArbiterHandle::wait`] for draining queued messages before
    /// stopping the system.
    pub fn start_with_handle<F>(threads: usize, factory: F) -> (Addr<A>, SyncArbiterHandle)
    where
        F: Fn() -> A + Send + Sync + 'static,
    {
        Self::start_with_thread_builder_and_handle(threads, thread::Builder::new, factory)
    }

    /// Start a new `SyncArbiter` with a single worker thread, creating the
    /// actor with a `FnOnce` factory.
    ///
    /// This is useful when the value used to construct the actor is not
    /// `Clone` or can only be consumed once, without having to wrap it in an
    /// `Arc` or `Option` yourself.
    ///
    /// Because the factory is only called once, actors started this way can
    /// not be restarted: calling [`ActorContext::stop`] or
    /// [`ActorContext::terminate`] on them will panic the worker thread.
    /// Use [`start`](Self::start) if the actor needs to be restarted.
    pub fn start_once<F>(factory: F) -> Addr<A>
    where
        F: FnOnce() -> A + Send + 'static,
    {
        Self::start_once_with_handle(factory).0
    }

    /// Start a new `SyncArbiter` with a single worker thread and a handle that
    /// can be used to wait for the worker to finish.
    ///
    /// See [`SyncArbiterHandle::wait`] for draining queued messages before
    /// stopping the system.
    pub fn start_once_with_handle<F>(factory: F) -> (Addr<A>, SyncArbiterHandle)
    where
        F: FnOnce() -> A + Send + 'static,
    {
        let (sender, receiver) = cb_channel::unbounded();
        let (tx, rx) = channel::channel(0);
        let sys = System::current();
        let actor_queue = receiver.clone();
        let inner_rx = rx.sender_producer();
        let state = Arc::new(WorkerState::new());

        state.add_worker();
        Self::spawn_worker(
            thread::Builder::new(),
            sys,
            Factory::once(factory),
            actor_queue,
            inner_rx,
            Arc::clone(&state),
        );

        System::current().arbiter().spawn(Self {
            queue: Some(sender),
            msgs: rx,
        });

        (Addr::new(tx), SyncArbiterHandle { state })
    }

    /// Start a new `SyncArbiter` with a single worker thread, using a fallible
    /// `FnOnce` factory.
    ///
    /// Unlike [`start_once`](Self::start_once), the factory is invoked on the
    /// calling thread, so initialization errors are returned to the caller
    /// instead of being raised inside the worker thread.
    ///
    /// Requires `A: Send` because the constructed actor is moved to the worker
    /// thread. As with `start_once`, the actor cannot be restarted.
    pub fn try_start_once<F, E>(factory: F) -> Result<Addr<A>, E>
    where
        F: FnOnce() -> Result<A, E>,
        A: Send + 'static,
    {
        let act = factory()?;
        Ok(Self::start_once(move || act))
    }

    /// Start a new `SyncArbiter` with specified number of worker threads.
    /// Each worker thread is spawned from the [`std::thread::Builder`]
    /// returned by a new call to `thread_builder_factory`.
    /// Returns a single address of the started actor. A single address is
    /// used to communicate to the actor(s), and messages are handled by
    /// the next available Actor in the `SyncArbiter`.
    pub fn start_with_thread_builder<F, BF>(
        threads: usize,
        thread_builder_factory: BF,
        factory: F,
    ) -> Addr<A>
    where
        F: Fn() -> A + Send + Sync + 'static,
        BF: FnMut() -> thread::Builder,
    {
        Self::start_with_thread_builder_and_handle(threads, thread_builder_factory, factory).0
    }

    /// Start a new `SyncArbiter` with specified number of worker threads and a
    /// handle that can be used to wait for all workers to finish.
    ///
    /// See [`SyncArbiterHandle::wait`] for draining queued messages before
    /// stopping the system.
    pub fn start_with_thread_builder_and_handle<F, BF>(
        threads: usize,
        mut thread_builder_factory: BF,
        factory: F,
    ) -> (Addr<A>, SyncArbiterHandle)
    where
        F: Fn() -> A + Send + Sync + 'static,
        BF: FnMut() -> thread::Builder,
    {
        let factory = Arc::new(factory);
        let (sender, receiver) = cb_channel::unbounded();
        let (tx, rx) = channel::channel(0);
        let state = Arc::new(WorkerState::new());

        for _ in 0..threads {
            let f = Arc::clone(&factory);
            let sys = System::current();
            let actor_queue = receiver.clone();
            let inner_rx = rx.sender_producer();

            state.add_worker();
            Self::spawn_worker(
                thread_builder_factory(),
                sys,
                Factory::repeat(f),
                actor_queue,
                inner_rx,
                Arc::clone(&state),
            );
        }

        System::current().arbiter().spawn(Self {
            queue: Some(sender),
            msgs: rx,
        });

        (Addr::new(tx), SyncArbiterHandle { state })
    }

    fn spawn_worker(
        builder: thread::Builder,
        sys: System,
        factory: Factory<A>,
        actor_queue: cb_channel::Receiver<Envelope<A>>,
        inner_rx: AddressSenderProducer<A>,
        state: Arc<WorkerState>,
    ) {
        builder
            .spawn(move || {
                let _guard = WorkerGuard(state);
                System::set_current(sys);
                SyncContext::new(factory, actor_queue, inner_rx).run();
            })
            .expect("failed to spawn thread");
    }
}

impl<A> Actor for SyncArbiter<A>
where
    A: Actor<Context = SyncContext<A>>,
{
    type Context = Context<Self>;
}

#[doc(hidden)]
impl<A> Future for SyncArbiter<A>
where
    A: Actor<Context = SyncContext<A>>,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            match Pin::new(&mut this.msgs).poll_next(cx) {
                Poll::Ready(Some(msg)) => {
                    if let Some(ref queue) = this.queue {
                        assert!(queue.send(msg).is_ok());
                    }
                }
                Poll::Pending => break,
                Poll::Ready(None) => unreachable!(),
            }
        }

        // stop condition
        if this.msgs.connected() {
            Poll::Pending
        } else {
            // stop sync arbiters
            this.queue = None;
            Poll::Ready(())
        }
    }
}

impl<A, M> ToEnvelope<A, M> for SyncContext<A>
where
    A: Actor<Context = Self> + Handler<M>,
    M: Message + Send + 'static,
    M::Result: Send,
{
    fn pack(msg: M, tx: Option<SyncSender<M::Result>>) -> Envelope<A> {
        Envelope::with_proxy(Box::new(SyncContextEnvelope::new(msg, tx)))
    }
}

/// Sync actor execution context. This is used instead of impl Actor for your Actor
/// instead of Context, if you intend this actor to run in a [`SyncArbiter`].
///
/// Unlike Context, an Actor that uses a [`SyncContext`] can not be stopped
/// by calling `stop` or `terminate`: Instead, these trigger a restart of
/// the Actor. Similar, returning `false` from `fn stopping` can not prevent
/// the restart or termination of the Actor.
///
/// Actors started with [`SyncArbiter::start_once`] can not be restarted,
/// because their factory can only be called once. Stopping or terminating
/// such an actor panics the worker thread.
///
/// # Examples
///
/// ```
/// use actix::prelude::*;
///
/// # struct Fibonacci(pub u32);
///
/// # impl Message for Fibonacci {
/// #     type Result = Result<u64, ()>;
/// # }
///
/// struct SyncActor;
///
/// impl Actor for SyncActor {
///     // It's important to note that you use "SyncContext" here instead of "Context".
///     type Context = SyncContext<Self>;
/// }
///
/// # fn main() {
/// # }
/// ```
pub struct SyncContext<A>
where
    A: Actor<Context = SyncContext<A>>,
{
    act: Option<A>,
    queue: cb_channel::Receiver<Envelope<A>>,
    stopping: bool,
    state: ActorState,
    factory: Factory<A>,
    address: AddressSenderProducer<A>,
}

enum Factory<A> {
    Repeat(Arc<dyn Fn() -> A + Send + Sync>),
    Once(Mutex<Option<Box<dyn FnOnce() -> A + Send>>>),
}

impl<A> Factory<A> {
    fn repeat(factory: Arc<dyn Fn() -> A + Send + Sync>) -> Self {
        Self::Repeat(factory)
    }

    fn once<F>(factory: F) -> Self
    where
        F: FnOnce() -> A + Send + 'static,
    {
        Self::Once(Mutex::new(Some(Box::new(factory))))
    }

    fn create(&self) -> A {
        match self {
            Self::Repeat(factory) => factory(),
            Self::Once(factory) => {
                let factory = factory
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .take()
                    .expect(
                        "SyncArbiter::start_once actor cannot be restarted: its factory was \
                         already consumed",
                    );
                factory()
            }
        }
    }
}

impl<A> SyncContext<A>
where
    A: Actor<Context = Self>,
{
    fn new(
        factory: Factory<A>,
        queue: cb_channel::Receiver<Envelope<A>>,
        address: AddressSenderProducer<A>,
    ) -> Self {
        let act = factory.create();
        Self {
            queue,
            factory,
            act: Some(act),
            stopping: false,
            state: ActorState::Started,
            address,
        }
    }

    fn run(&mut self) {
        let mut act = self.act.take().unwrap();

        // started
        A::started(&mut act, self);
        self.state = ActorState::Running;

        loop {
            match self.queue.recv() {
                Ok(mut env) => {
                    env.handle(&mut act, self);
                }
                Err(_) => {
                    self.state = ActorState::Stopping;
                    if A::stopping(&mut act, self) != Running::Stop {
                        warn!("stopping method is not supported for sync actors");
                    }
                    self.state = ActorState::Stopped;
                    A::stopped(&mut act, self);
                    return;
                }
            }

            if self.stopping {
                self.stopping = false;

                // stop old actor
                A::stopping(&mut act, self);
                self.state = ActorState::Stopped;
                A::stopped(&mut act, self);

                // start new actor
                self.state = ActorState::Started;
                act = self.factory.create();
                A::started(&mut act, self);
                self.state = ActorState::Running;
            }
        }
    }

    pub fn address(&self) -> Addr<A> {
        Addr::new(self.address.sender())
    }
}

impl<A> ActorContext for SyncContext<A>
where
    A: Actor<Context = Self>,
{
    /// Stop the current Actor. [`SyncContext`] will stop the existing Actor, and restart
    /// a new Actor of the same type to replace it.
    fn stop(&mut self) {
        self.stopping = true;
        self.state = ActorState::Stopping;
    }

    /// Terminate the current Actor. [`SyncContext`] will terminate the existing Actor, and restart
    /// a new Actor of the same type to replace it.
    fn terminate(&mut self) {
        self.stopping = true;
        self.state = ActorState::Stopping;
    }

    /// Get the Actor execution state.
    fn state(&self) -> ActorState {
        self.state
    }
}

pub(crate) struct SyncContextEnvelope<M>
where
    M: Message + Send,
{
    msg: Option<M>,
    tx: Option<SyncSender<M::Result>>,
}

impl<M> SyncContextEnvelope<M>
where
    M: Message + Send,
    M::Result: Send,
{
    pub fn new(msg: M, tx: Option<SyncSender<M::Result>>) -> Self {
        Self { tx, msg: Some(msg) }
    }
}

impl<A, M> EnvelopeProxy<A> for SyncContextEnvelope<M>
where
    M: Message + Send + 'static,
    M::Result: Send,
    A: Actor<Context = SyncContext<A>> + Handler<M>,
{
    fn handle(&mut self, act: &mut A, ctx: &mut A::Context) {
        let tx = self.tx.take();
        if tx.is_some() && tx.as_ref().unwrap().is_closed() {
            return;
        }

        if let Some(msg) = self.msg.take() {
            <A as Handler<M>>::handle(act, msg, ctx).handle(ctx, tx)
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::oneshot;

    use crate::prelude::*;

    struct SyncActor2;

    impl Actor for SyncActor2 {
        type Context = SyncContext<Self>;
    }

    struct SyncActor1(Addr<SyncActor2>);

    impl Actor for SyncActor1 {
        type Context = SyncContext<Self>;
    }

    impl SyncActor1 {
        fn run() -> SyncActor1 {
            SyncActor1(SyncArbiter::start(1, || SyncActor2))
        }
    }

    struct Msg(oneshot::Sender<u8>);

    impl Message for Msg {
        type Result = ();
    }

    impl Handler<Msg> for SyncActor1 {
        type Result = ();

        fn handle(&mut self, msg: Msg, _: &mut Self::Context) -> Self::Result {
            self.0.do_send(msg);
        }
    }

    impl Handler<Msg> for SyncActor2 {
        type Result = ();

        fn handle(&mut self, msg: Msg, _: &mut Self::Context) -> Self::Result {
            msg.0.send(233u8).unwrap();
        }
    }

    #[test]
    fn nested_sync_arbiters() {
        System::new().block_on(async {
            let addr = SyncArbiter::start(1, SyncActor1::run);
            let (tx, rx) = oneshot::channel();
            addr.send(Msg(tx)).await.unwrap();
            assert_eq!(233u8, rx.await.unwrap());
            System::current().stop();
        })
    }
}
