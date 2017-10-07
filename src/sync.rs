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
//! extern crate actix;
//! extern crate futures;
//!
//! use actix::prelude::*;
//!
//! struct Fibonacci(pub u32);
//!
//! struct SyncActor;
//!
//! impl Actor for SyncActor {
//!     type Context = SyncContext<Self>;
//! }
//!
//! impl ResponseType<Fibonacci> for SyncActor {
//!     type Item = u64;
//!     type Error = ();
//! }
//!
//! impl Handler<Fibonacci> for SyncActor {
//!     fn handle(&mut self, msg: Fibonacci, _: &mut Self::Context) -> Response<Self, Fibonacci> {
//!         if msg.0 == 0 {
//!             Self::reply_error(())
//!         } else if msg.0 == 1 {
//!             Self::reply(1)
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
//!             Self::reply(sum)
//!         }
//!    }
//! }
//!
//! fn main() {
//!     let sys = System::new("test".to_owned());
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
//!         Arbiter::system().send(msgs::SystemExit(0));
//!         futures::future::result(Ok(()))
//!     });
//!
//!     sys.run();
//! }
//! ```
use std;
use std::thread;
use std::sync::Arc;

use crossbeam::sync::MsQueue;
use futures::{Async, Future, Poll, Stream};
use futures::sync::oneshot::Sender as SyncSender;
use tokio_core::reactor::Core;

use actor::{Actor, ActorContext, ActorState, Handler, ResponseType};
use arbiter::Arbiter;
use address::SyncAddress;
use context::Context;
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
    pub fn start<F>(threads: usize, f: F) -> SyncAddress<A>
        where F: Fn() -> A + 'static
    {
        let queue = Arc::new(MsQueue::new());

        for _ in 0..threads {
            let actor = f();
            let actor_queue = Arc::clone(&queue);

            thread::spawn(move || {
                SyncContext::new(actor, actor_queue)
                    .run()
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

impl<A, M> ToEnvelope<A, SyncContext<A>, M> for A
    where A: Actor<Context=SyncContext<A>> + Handler<M>,
          M: Send + 'static,
          A::Item: Send,
          A::Error: Send
{
    fn pack(msg: M, tx: Option<SyncSender<Result<A::Item, A::Error>>>) -> Envelope<A>
    {
        Envelope::new(SyncEnvelope::new(msg, tx))
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
}

impl<A> SyncContext<A> where A: Actor<Context=Self> {
    /// Create new SyncContext
    fn new(act: A, queue: Arc<MsQueue<SyncContextProtocol<A>>>) -> Self {
        SyncContext {
            act: act,
            core: None,
            queue: queue,
            stopping: false,
            state: ActorState::Started,
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
                    env.handle(&mut self.act, ctx)
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
}

impl<A> ActorContext<A> for SyncContext<A> where A: Actor<Context=Self>
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
    where A: Actor<Context=SyncContext<A>> + Handler<M>,
{
    msg: Option<M>,
    tx: Option<SyncSender<Result<A::Item, A::Error>>>,
}

impl<A, M>  SyncEnvelope<A, M>
    where A: Actor<Context=SyncContext<A>> + Handler<M>
{
    pub fn new(msg: M, tx: Option<SyncSender<Result<A::Item, A::Error>>>) -> Self {
        SyncEnvelope{msg: Some(msg), tx: tx}
    }
}

impl<A, M> EnvelopeProxy for SyncEnvelope<A, M>
    where M: 'static,
          A: Actor<Context=SyncContext<A>> + Handler<M>,
{
    type Actor = A;

    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context)
    {
        if let Some(msg) = self.msg.take() {
            let mut response = <Self::Actor as Handler<M>>::handle(act, msg, ctx);

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


struct ResponseFuture<A, M> where A: Actor<Context=SyncContext<A>> + ResponseType<M> {
    fut: Response<A, M>,
    act: *mut A,
    ctx: *mut SyncContext<A>
}


impl<A, M> Future for ResponseFuture<A, M>
    where A: Actor<Context=SyncContext<A>> + ResponseType<M>
{
    type Item = A::Item;
    type Error = A::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
    {
        let act = unsafe{ &mut *self.act };
        let ctx = unsafe{ &mut *self.ctx };

        self.fut.poll(act, ctx)
    }
}
