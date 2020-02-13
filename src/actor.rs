use std::time::Duration;

use actix_rt::Arbiter;
use futures_util::stream::Stream;
use log::error;

use crate::address::{channel, Addr};
use crate::context::Context;
use crate::contextitems::{
    ActorDelayedMessageItem, ActorMessageItem, ActorMessageStreamItem,
};
use crate::fut::{ActorFuture, ActorStream};
use crate::handler::{Handler, Message};
use crate::mailbox::DEFAULT_CAPACITY;
use crate::stream::StreamHandler;
use crate::utils::{IntervalFunc, TimerFunc};

#[allow(unused_variables)]
/// Actors are objects which encapsulate state and behavior.
///
/// Actors run within a specific execution context
/// [`Context<A>`](struct.Context.html). The context object is available
/// only during execution. Each actor has a separate execution
/// context. The execution context also controls the lifecycle of an
/// actor.
///
/// Actors communicate exclusively by exchanging messages. The sender
/// actor can wait for a response. Actors are not referenced directly,
/// but by address [`Addr`](struct.Addr.html) To be able to handle a
/// specific message actor has to provide a
/// [`Handler<M>`](trait.Handler.html) implementation for this
/// message. All messages are statically typed. A message can be
/// handled in asynchronous fashion. An actor can spawn other actors
/// or add futures or streams to the execution context. The actor
/// trait provides several methods that allow controlling the actor
/// lifecycle.
///
/// # Actor lifecycle
///
/// ## Started
///
/// An actor starts in the `Started` state, during this state the
/// `started` method gets called.
///
/// ## Running
///
/// After an actor's `started` method got called, the actor
/// transitions to the `Running` state. An actor can stay in the
/// `running` state for an indefinite amount of time.
///
/// ## Stopping
///
/// The actor's execution state changes to `stopping` in the following
/// situations:
///
/// * `Context::stop` gets called by actor itself
/// * all addresses to the actor get dropped
/// * no evented objects are registered in its context.
///
/// An actor can return from the `stopping` state to the `running`
/// state by creating a new address or adding an evented object, like
/// a future or stream, in its `Actor::stopping` method.
///
/// If an actor changed to a `stopping` state because
/// `Context::stop()` got called, the context then immediately stops
/// processing incoming messages and calls the `Actor::stopping()`
/// method. If an actor does not return back to a `running` state,
/// all unprocessed messages get dropped.
///
/// ## Stopped
///
/// If an actor does not modify execution context while in stopping
/// state, the actor state changes to `Stopped`. This state is
/// considered final and at this point the actor gets dropped.
///
pub trait Actor: Sized + Unpin + 'static {
    /// Actor execution context type
    type Context: ActorContext;

    /// Called when an actor gets polled the first time.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Called after an actor is in `Actor::Stopping` state.
    ///
    /// There can be several reasons for stopping:
    ///
    /// - `Context::stop` gets called by the actor itself.
    /// - All addresses to the current actor get dropped and no more
    ///   evented objects are left in the context.
    ///
    /// An actor can return from the stopping state to the running
    /// state by returning `Running::Continue`.
    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        Running::Stop
    }

    /// Called after an actor is stopped.
    ///
    /// This method can be used to perform any needed cleanup work or
    /// to spawn more actors. This is the final state, after this
    /// method got called, the actor will be dropped.
    fn stopped(&mut self, ctx: &mut Self::Context) {}

    /// Start a new asynchronous actor, returning its address.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use actix::*;
    ///
    /// struct MyActor;
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    /// }
    ///
    /// fn main() {
    ///     // initialize system
    ///     System::run(|| {
    ///         let addr = MyActor.start(); // <- start actor and get its address
    /// #       System::current().stop();
    ///     });
    /// }
    /// ```
    fn start(self) -> Addr<Self>
    where
        Self: Actor<Context = Context<Self>>,
    {
        Context::new().run(self)
    }

    /// Construct and start a new asynchronous actor, returning its
    /// address.
    ///
    /// This is constructs a new actor using the `Default` trait, and
    /// invokes its `start` method.
    fn start_default() -> Addr<Self>
    where
        Self: Actor<Context = Context<Self>> + Default,
    {
        Self::default().start()
    }

    /// Start new actor in arbiter's thread.
    fn start_in_arbiter<F>(arb: &Arbiter, f: F) -> Addr<Self>
    where
        Self: Actor<Context = Context<Self>>,
        F: FnOnce(&mut Context<Self>) -> Self + Send + 'static,
    {
        let (tx, rx) = channel::channel(DEFAULT_CAPACITY);

        // create actor
        arb.exec_fn(move || {
            let mut ctx = Context::with_receiver(rx);
            let act = f(&mut ctx);
            let fut = ctx.into_future(act);

            actix_rt::spawn(fut);
        });

        Addr::new(tx)
    }

    /// Start a new asynchronous actor given a `Context`.
    ///
    /// Use this method if you need the `Context` object during actor
    /// initialization.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use actix::*;
    ///
    /// struct MyActor {
    ///     val: usize,
    /// }
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    /// }
    ///
    /// fn main() {
    ///     // initialize system
    ///     System::run(|| {
    ///         let addr = MyActor::create(|ctx: &mut Context<MyActor>| MyActor { val: 10 });
    /// #       System::current().stop();
    ///     });
    /// }
    /// ```
    fn create<F>(f: F) -> Addr<Self>
    where
        Self: Actor<Context = Context<Self>>,
        F: FnOnce(&mut Context<Self>) -> Self,
    {
        let mut ctx = Context::new();
        let act = f(&mut ctx);
        ctx.run(act)
    }
}

#[allow(unused_variables)]
/// Actors with the ability to restart after failure.
///
/// Supervised actors can be managed by a
/// [`Supervisor`](struct.Supervisor.html). As an additional lifecycle
/// event, the `restarting` method can be implemented.
///
/// If a supervised actor fails, its supervisor creates new execution
/// context and restarts the actor, invoking its `restarting` method.
/// After a call to this method, the actor's execution state changes
/// to `Started` and the regular lifecycle process starts.
///
/// The `restarting` method gets called with the newly constructed
/// `Context` object.
pub trait Supervised: Actor {
    /// Called when the supervisor restarts a failed actor.
    fn restarting(&mut self, ctx: &mut <Self as Actor>::Context) {}
}

/// Actor execution state
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum ActorState {
    /// Actor is started.
    Started,
    /// Actor is running.
    Running,
    /// Actor is stopping.
    Stopping,
    /// Actor is stopped.
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Running {
    Stop,
    Continue,
}

impl ActorState {
    /// Indicates whether the actor is alive.
    pub fn alive(self) -> bool {
        self == ActorState::Started || self == ActorState::Running
    }
    /// Indicates whether the actor is stopped or stopping.
    pub fn stopping(self) -> bool {
        self == ActorState::Stopping || self == ActorState::Stopped
    }
}

/// Actor execution context.
///
/// Each actor runs within a specific execution context. The actor's
/// associated type `Actor::Context` defines the context to use for
/// the actor, and must implement the `ActorContext` trait.
///
/// The execution context defines the type of execution, and the
/// actor's communication channels (message handling).
pub trait ActorContext: Sized {
    /// Immediately stop processing incoming messages and switch to a
    /// `stopping` state. This only affects actors that are currently
    /// `running`. Future attempts to queue messages will fail.
    fn stop(&mut self);

    /// Terminate actor execution unconditionally. This sets the actor
    /// into the `stopped` state. This causes future attempts to queue
    /// messages to fail.
    fn terminate(&mut self);

    /// Retrieve the current Actor execution state.
    fn state(&self) -> ActorState;
}

/// Asynchronous execution context.
pub trait AsyncContext<A>: ActorContext
where
    A: Actor<Context = Self>,
{
    /// Returns the address of the context.
    fn address(&self) -> Addr<A>;

    /// Spawns a future into the context.
    ///
    /// Returns a handle of the spawned future, which can be used for
    /// cancelling its execution.
    ///
    /// All futures spawned into an actor's context are cancelled
    /// during the actor's stopping stage.
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
    where
        F: ActorFuture<Output = (), Actor = A> + 'static;

    /// Spawns a future into the context, waiting for it to resolve.
    ///
    /// This stops processing any incoming events until the future
    /// resolves.
    fn wait<F>(&mut self, fut: F)
    where
        F: ActorFuture<Output = (), Actor = A> + 'static;

    /// Checks if the context is paused (waiting for future completion or stopping).
    fn waiting(&self) -> bool;

    /// Cancels a spawned future.
    ///
    /// The `handle` is a value returned by the `spawn` method.
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool;

    /// Registers a stream with the context.
    ///
    /// This allows handling a `Stream` in a way similar to normal
    /// actor messages.
    ///
    /// ```rust
    /// # use std::io;
    /// use actix::prelude::*;
    /// use futures_util::stream::once;
    ///
    /// #[derive(Message)]
    /// #[rtype(result = "()")]
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl StreamHandler<Ping> for MyActor {
    ///
    ///     fn handle(&mut self, item: Ping, ctx: &mut Context<MyActor>) {
    ///         println!("PING");
    ///         System::current().stop();
    ///     }
    ///
    ///     fn finished(&mut self, ctx: &mut Self::Context) {
    ///         println!("finished");
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///    type Context = Context<Self>;
    ///
    ///    fn started(&mut self, ctx: &mut Context<Self>) {
    ///        // add stream
    ///        ctx.add_stream(once(async { Ping }));
    ///    }
    /// }
    ///
    /// fn main() {
    ///     let sys = System::new("example");
    ///     let addr = MyActor.start();
    ///     sys.run();
    ///  }
    /// ```
    fn add_stream<S>(&mut self, fut: S) -> SpawnHandle
    where
        S: Stream + 'static,
        A: StreamHandler<S::Item>,
    {
        <A as StreamHandler<S::Item>>::add_stream(fut, self)
    }

    /// Registers a stream with the context, ignoring errors.
    ///
    /// This method is similar to `add_stream` but it skips stream
    /// errors.
    ///
    /// ```rust
    /// use actix::prelude::*;
    /// use futures_util::stream::once;
    ///
    /// #[derive(Message)]
    /// #[rtype(result = "()")]
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl Handler<Ping> for MyActor {
    ///     type Result = ();
    ///
    ///     fn handle(&mut self, msg: Ping, ctx: &mut Context<MyActor>) {
    ///         println!("PING");
    /// #       System::current().stop();
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    ///
    ///     fn started(&mut self, ctx: &mut Context<Self>) {
    ///         // add messages stream
    ///         ctx.add_message_stream(once(async { Ping }));
    ///     }
    /// }
    ///
    /// fn main() {
    ///    System::run(|| {
    ///        let addr = MyActor.start();
    ///    });
    /// }
    /// ```
    fn add_message_stream<S>(&mut self, fut: S)
    where
        S: Stream + 'static,
        S::Item: Message,
        A: Handler<S::Item>,
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_message_stream called for stopped actor.");
        } else {
            self.spawn(ActorMessageStreamItem::new(fut));
        }
    }

    /// Sends the message `msg` to self. This bypasses the mailbox capacity, and
    /// will always queue the message. If the actor is in the `stopped` state, an
    /// error will be raised.
    fn notify<M>(&mut self, msg: M)
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        if self.state() == ActorState::Stopped {
            error!("Context::notify called for stopped actor.");
        } else {
            self.spawn(ActorMessageItem::new(msg));
        }
    }

    /// Sends the message `msg` to self after a specified period of time.
    ///
    /// Returns a spawn handle which can be used for cancellation. The
    /// notification gets cancelled if the context's stop method gets
    /// called. This bypasses the mailbox capacity, and
    /// will always queue the message. If the actor is in the `stopped` state, an
    /// error will be raised.
    fn notify_later<M>(&mut self, msg: M, after: Duration) -> SpawnHandle
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        if self.state() == ActorState::Stopped {
            error!("Context::notify_later called for stopped actor.");
            SpawnHandle::default()
        } else {
            self.spawn(ActorDelayedMessageItem::new(msg, after))
        }
    }

    /// Executes a closure after a specified period of time.
    ///
    /// The closure gets passed the same actor and its
    /// context. Execution gets cancelled if the context's stop method
    /// gets called.
    fn run_later<F>(&mut self, dur: Duration, f: F) -> SpawnHandle
    where
        F: FnOnce(&mut A, &mut A::Context) + 'static,
    {
        self.spawn(TimerFunc::new(dur, f))
    }

    /// Spawns a job to execute the given closure periodically, at a
    /// specified fixed interval.
    fn run_interval<F>(&mut self, dur: Duration, f: F) -> SpawnHandle
    where
        F: FnMut(&mut A, &mut A::Context) + 'static,
    {
        self.spawn(IntervalFunc::new(dur, f).finish())
    }
}

/// A handle to a spawned future.
///
/// Can be used to cancel the future.
#[derive(Eq, PartialEq, Debug, Copy, Clone, Hash)]
pub struct SpawnHandle(usize);

impl SpawnHandle {
    /// Gets the next handle.
    pub fn next(self) -> SpawnHandle {
        SpawnHandle(self.0 + 1)
    }
    #[doc(hidden)]
    pub fn into_usize(self) -> usize {
        self.0
    }
}

impl Default for SpawnHandle {
    fn default() -> SpawnHandle {
        SpawnHandle(0)
    }
}
