//! Sync Actors support
//!
//! Sync Actors are actors that run multiple instances on a thread pool.
//! This is useful for CPU bound, or concurrent workloads. There can only be
//! a single Sync Actor type on a `SyncArbiter`. This means you can't have
//! Actor type A and B, sharing the same thread pool. You need to create two
//! SyncArbiters and have A and B spawn on unique `SyncArbiter`s respectively.
//! For more information and examples, see `SyncArbiter`
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::{task, thread};

use actix_rt::System;
use crossbeam_channel as cb_channel;
use futures_channel::oneshot::Sender as SyncSender;
use futures_util::{future::Future, stream::StreamExt};
use log::warn;
use pin_project::pin_project;

use crate::actor::{Actor, ActorContext, ActorState, Running};
use crate::address::channel;
use crate::address::{
    Addr, AddressReceiver, AddressSenderProducer, Envelope, EnvelopeProxy, ToEnvelope,
};
use crate::context::Context;
use crate::handler::{Handler, Message, MessageResponse};

/// SyncArbiter provides the resources for a single Sync Actor to run on a dedicated
/// thread or threads. This is generally used for CPU bound concurrent workloads. It's
/// important to note, that the SyncArbiter generates a single address for the pool
/// of hosted Sync Actors. Any message sent to this Address, will be operated on by
/// a single Sync Actor from the pool.
///
/// Sync Actors have a different lifecycle compared to Actors on the System
/// Arbiter. For more, see `SyncContext`.
///
/// ## Example
///
/// ```rust
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
///     System::run(|| {
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
#[pin_project]
pub struct SyncArbiter<A>
where
    A: Actor<Context = SyncContext<A>>,
{
    queue: Option<cb_channel::Sender<Envelope<A>>>,
    msgs: AddressReceiver<A>,
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
        let factory = Arc::new(factory);
        let (sender, receiver) = cb_channel::unbounded();
        let (tx, rx) = channel::channel(0);

        for _ in 0..threads {
            let f = Arc::clone(&factory);
            let sys = System::current();
            let actor_queue = receiver.clone();
            let inner_rx = rx.sender_producer();

            thread::spawn(move || {
                System::set_current(sys);
                SyncContext::new(f, actor_queue, inner_rx).run();
            });
        }

        actix_rt::spawn(Self {
            queue: Some(sender),
            msgs: rx,
        });

        Addr::new(tx)
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
        let this = self.project();
        loop {
            match this.msgs.poll_next_unpin(cx) {
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
            *this.queue = None;
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
/// instead of Context, if you intend this actor to run in a SyncArbiter.
///
/// Unlike Context, an Actor that uses a SyncContext can not be stopped
/// by calling `stop` or `terminate`: Instead, these trigger a restart of
/// the Actor. Similar, returning `false` from `fn stopping` can not prevent
/// the restart or termination of the Actor.
///
/// ## Example
///
/// ```rust
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
    factory: Arc<dyn Fn() -> A>,
    address: AddressSenderProducer<A>,
}

impl<A> SyncContext<A>
where
    A: Actor<Context = Self>,
{
    fn new(
        factory: Arc<dyn Fn() -> A>,
        queue: cb_channel::Receiver<Envelope<A>>,
        address: AddressSenderProducer<A>,
    ) -> Self {
        let act = factory();
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
                act = (*self.factory)();
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
    /// Stop the current Actor. SyncContext will stop the existing Actor, and restart
    /// a new Actor of the same type to replace it.
    fn stop(&mut self) {
        self.stopping = true;
        self.state = ActorState::Stopping;
    }

    /// Terminate the current Actor. SyncContext will terminate the existing Actor, and restart
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

pub(crate) struct SyncContextEnvelope<A, M>
where
    A: Actor<Context = SyncContext<A>> + Handler<M>,
    M: Message + Send,
{
    msg: Option<M>,
    tx: Option<SyncSender<M::Result>>,
    actor: PhantomData<A>,
}

unsafe impl<A, M> Send for SyncContextEnvelope<A, M>
where
    A: Actor<Context = SyncContext<A>> + Handler<M>,
    M: Message + Send,
{
}

impl<A, M> SyncContextEnvelope<A, M>
where
    A: Actor<Context = SyncContext<A>> + Handler<M>,
    M: Message + Send,
    M::Result: Send,
{
    pub fn new(msg: M, tx: Option<SyncSender<M::Result>>) -> Self {
        Self {
            tx,
            msg: Some(msg),
            actor: PhantomData,
        }
    }
}

impl<A, M> EnvelopeProxy for SyncContextEnvelope<A, M>
where
    M: Message + Send + 'static,
    M::Result: Send,
    A: Actor<Context = SyncContext<A>> + Handler<M>,
{
    type Actor = A;

    fn handle(&mut self, act: &mut A, ctx: &mut A::Context) {
        let tx = self.tx.take();
        if tx.is_some() && tx.as_ref().unwrap().is_canceled() {
            return;
        }

        if let Some(msg) = self.msg.take() {
            <A as Handler<M>>::handle(act, msg, ctx).handle(ctx, tx)
        }
    }
}
