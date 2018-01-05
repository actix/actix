//! Sync actors support
//!
//! Sync actors could be used for cpu bound behavior. Only one sync actor
//! runs within arbiter's thread. Sync actor process one message at a time.
//! Sync arbiter can start mutiple threads with separate instance of actor in each.
//! Multi consummer queue is used as a communication channel queue.
//! To be able to start sync actor via `SyncArbiter`
//! Actor has to use `SyncContext` as an execution context.
//!
//! ## Example
//!
//! ```rust
//! # extern crate actix;
//! # extern crate futures;
//! use actix::prelude::*;
//!
//! struct Fibonacci(pub u32);
//!
//! impl ResponseType for Fibonacci {
//!     type Item = u64;
//!     type Error = ();
//! }
//!
//! struct SyncActor;
//!
//! impl Actor for SyncActor {
//!     type Context = SyncContext<Self>;
//! }
//!
//! impl Handler<Fibonacci> for SyncActor {
//!     type Result = MessageResult<Fibonacci>;
//!
//!     fn handle(&mut self, msg: Fibonacci, _: &mut Self::Context) -> Self::Result {
//!         if msg.0 == 0 {
//!             Err(())
//!         } else if msg.0 == 1 {
//!             Ok(1)
//!         } else {
//!             let mut i = 0;
//!             let mut sum = 0;
//!             let mut last = 0;
//!             let mut curr = 1;
//!             while i < msg.0 - 1 {
//!                 sum = last + curr;
//!                 last = curr;
//!                 curr = sum;
//!                 i += 1;
//!             }
//!             Ok(sum)
//!         }
//!    }
//! }
//!
//! fn main() {
//!     let sys = System::new("test");
//!
//!     // start sync arbiter with 2 threads
//!     let addr = SyncArbiter::start(2, || SyncActor);
//!
//!     // send 5 messages
//!     for n in 5..10 {
//!         addr.send(Fibonacci(n));
//!     }
//!
//!     Arbiter::handle().spawn_fn(|| {
//! #        Arbiter::system().send(actix::msgs::SystemExit(0));
//!         futures::future::result(Ok(()))
//!     });
//!
//!     sys.run();
//! }
//! ```
use std;
use std::thread;
use std::sync::Arc;
use std::marker::PhantomData;

use crossbeam::sync::MsQueue;
use futures::{Async, Future, Poll, Stream};
use futures::sync::oneshot::Sender as SyncSender;
use tokio_core::reactor::Core;

use actor::{Actor, ActorContext, ActorState};
use arbiter::Arbiter;
use address::SyncAddress;
use context::Context;
use handler::{Handler, ResponseType, IntoResponse};
use envelope::{Envelope, EnvelopeProxy, ToEnvelope};
use message::Response;
use queue::sync;

/// Sync arbiter
pub struct SyncArbiter<A> where A: Actor<Context=SyncContext<A>> {
    queue: Arc<MsQueue<SyncContextProtocol<A>>>,
    msgs: sync::UnboundedReceiver<Envelope<A>>,
    threads: usize,
}

impl<A> SyncArbiter<A> where A: Actor<Context=SyncContext<A>> + Send {

    /// Start new sync arbiter with specified number of worker threads.
    /// Returns address of started actor.
    pub fn start<F>(threads: usize, factory: F) -> SyncAddress<A>
        where F: Sync + Send + Fn() -> A + 'static
    {
        let factory = Arc::new(factory);
        let queue = Arc::new(MsQueue::new());

        for _ in 0..threads {
            let f = Arc::clone(&factory);
            let actor_queue = Arc::clone(&queue);

            thread::spawn(move || {
                SyncContext::new(f, actor_queue).run()
            });
        }

        let (tx, rx) = sync::unbounded();
        Arbiter::handle().spawn(
            SyncArbiter{queue: queue, msgs: rx, threads: threads});

        SyncAddress::new(tx)
    }
}

impl<A> Actor for SyncArbiter<A> where A: Actor<Context=SyncContext<A>>{
    type Context = Context<Self>;
}

#[doc(hidden)]
impl<A> Future for SyncArbiter<A> where A: Actor<Context=SyncContext<A>>
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.msgs.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    self.queue.push(SyncContextProtocol::Envelope(msg));
                }
                Ok(Async::NotReady) => break,
                Ok(Async::Ready(None)) | Err(_) => {
                    // stop sync arbiters
                    for _ in 0..self.threads {
                        self.queue.push(SyncContextProtocol::Stop);
                    }
                    return Ok(Async::Ready(()))
                },
            }
        }
        Ok(Async::NotReady)
    }
}

impl<A> ToEnvelope<A> for SyncContext<A>
    where A: Actor<Context=SyncContext<A>>,
{
    fn pack<M>(msg: M,
               tx: Option<SyncSender<Result<M::Item, M::Error>>>,
               cancel_on_drop: bool) -> Envelope<A>
        where A: Handler<M>,
              M: ResponseType + Send + 'static,
              <M as ResponseType>::Item: Send,
              <M as ResponseType>::Error: Send
    {
        Envelope::new(SyncEnvelope::new(msg, tx, cancel_on_drop))
    }
}

enum SyncContextProtocol<A> where A: Actor<Context=SyncContext<A>> {
    Stop,
    Envelope(Envelope<A>),
}

/// Sync actor execution context
pub struct SyncContext<A> where A: Actor<Context=SyncContext<A>> {
    act: A,
    core: Option<Core>,
    queue: Arc<MsQueue<SyncContextProtocol<A>>>,
    stopping: bool,
    state: ActorState,
    factory: Arc<Fn() -> A + Send + Sync>,
    restart: bool,
}

impl<A> SyncContext<A> where A: Actor<Context=Self> {
    /// Create new SyncContext
    fn new(factory: Arc<Fn() -> A + Send + Sync>,
           queue: Arc<MsQueue<SyncContextProtocol<A>>>) -> Self {
        SyncContext {
            act: factory(),
            core: None,
            queue: queue,
            stopping: false,
            state: ActorState::Started,
            factory: factory,
            restart: false,
        }
    }

    fn run(&mut self) {
        let ctx: &mut SyncContext<A> = unsafe {
            std::mem::transmute(self as &mut SyncContext<A>)
        };

        // started
        A::started(&mut self.act, ctx);
        self.state = ActorState::Running;

        loop {
            match self.queue.pop() {
                SyncContextProtocol::Stop => {
                    self.state = ActorState::Stopping;
                    A::stopping(&mut self.act, ctx);
                    self.state = ActorState::Stopped;
                    A::stopped(&mut self.act, ctx);
                    return
                },
                SyncContextProtocol::Envelope(mut env) => {
                    env.handle(&mut self.act, ctx);

                    if self.restart {
                        self.restart = false;
                        self.stopping = false;

                        // stop old actor
                        A::stopping(&mut self.act, ctx);
                        self.state = ActorState::Stopped;
                        A::stopped(&mut self.act, ctx);

                        // start new actor
                        self.state = ActorState::Started;
                        self.act = (*self.factory)();
                        A::started(&mut self.act, ctx);
                        self.state = ActorState::Running;
                    }
                },
            }

            if self.stopping {
                A::stopping(&mut self.act, ctx);
                self.state = ActorState::Stopped;
                A::stopped(&mut self.act, ctx);
                return
            }
        }
    }

    /// Initiate actor restart process.
    pub fn restart(&mut self) {
        self.restart = true;
    }
}

impl<A> ActorContext for SyncContext<A> where A: Actor<Context=Self>
{
    /// Stop actor execution
    fn stop(&mut self) {
        self.stopping = true;
        self.state = ActorState::Stopping;
    }

    /// Terminate actor execution
    fn terminate(&mut self) {
        self.stopping = true;
        self.state = ActorState::Stopping;
    }

    /// Actor execution state
    fn state(&self) -> ActorState {
        self.state
    }
}

pub(crate) struct SyncEnvelope<A, M>
    where A: Actor<Context=SyncContext<A>> + Handler<M>, M: ResponseType,
{
    msg: Option<M>,
    tx: Option<SyncSender<Result<M::Item, M::Error>>>,
    actor: PhantomData<A>,
    cancel_on_drop: bool,
}

impl<A, M>  SyncEnvelope<A, M>
    where A: Actor<Context=SyncContext<A>> + Handler<M>,
          M: ResponseType,
{
    pub fn new(msg: M, tx: Option<SyncSender<Result<M::Item, M::Error>>>,
               cancel_on_drop: bool) -> Self {
        SyncEnvelope{msg: Some(msg),
                     tx: tx,
                     actor: PhantomData,
                     cancel_on_drop: cancel_on_drop}
    }
}

impl<A, M> EnvelopeProxy for SyncEnvelope<A, M>
    where M: ResponseType + 'static,
          A: Actor<Context=SyncContext<A>> + Handler<M>,
{
    type Actor = A;

    fn handle(&mut self, act: &mut A, ctx: &mut A::Context)
    {
        let tx = self.tx.take();
        if tx.is_some() && self.cancel_on_drop && tx.as_ref().unwrap().is_canceled() {
            return
        }

        if let Some(msg) = self.msg.take() {
            let mut response = <A as Handler<M>>::handle(act, msg, ctx).into_response();

            let result = if response.is_async() {
                response.result().unwrap()
            } else {
                if ctx.core.is_none() {
                    ctx.core = Some(Core::new().unwrap());
                }

                let ctx_ptr = ctx as *mut _;
                let core = ctx.core.as_mut().unwrap();
                core.run(ResponseFuture{
                    fut: response,
                    act: act as *mut _,
                    ctx: ctx_ptr})
            };

            if let Some(tx) = self.tx.take() {
                let _ = tx.send(result);
            }
        }
    }
}


struct ResponseFuture<A, M> where A: Actor<Context=SyncContext<A>>, M: ResponseType {
    fut: Response<A, M>,
    act: *mut A,
    ctx: *mut SyncContext<A>
}


impl<A, M> Future for ResponseFuture<A, M>
    where A: Actor<Context=SyncContext<A>>,
          M: ResponseType
{
    type Item = M::Item;
    type Error = M::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
    {
        let act = unsafe{ &mut *self.act };
        let ctx = unsafe{ &mut *self.ctx };

        self.fut.poll_response(act, ctx)
    }
}
