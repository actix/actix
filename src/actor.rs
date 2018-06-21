use std::time::Duration;

use futures::Stream;

use address::Addr;
use arbiter::Arbiter;
use context::Context;
use contextitems::{ActorDelayedMessageItem, ActorMessageItem, ActorMessageStreamItem};
use fut::{ActorFuture, ActorStream};
use handler::{Handler, Message};
use stream::StreamHandler;
use stream2::StreamHandler2;
use utils::{IntervalFunc, TimerFunc};

#[allow(unused_variables)]
/// Actors are objects which encapsulate state and behavior.
///
/// Actors run within specific execution context
/// [Context<A>](struct.Context.html).
/// Context object is available only during execution. Each actor has separate
/// execution context. Also execution context controls lifecycle of an actor.
///
/// Actors communicate exclusively by exchanging messages. Sender actor can
/// wait for response. Actors are not referenced directly, but by
/// address [`Addr`](struct.Addr.html)
/// To be able to handle specific message actor has to provide
/// [`Handler<M>`](trait.Handler.html)
/// implementation for this message. All messages are statically typed. Message
/// could be handled in asynchronous fashion. Actor can spawn other actors or
/// add futures or streams to execution context. Actor trait provides several
/// methods that allow to control actor lifecycle.
///
/// # Actor lifecycle
///
/// ## Started
///
/// Actor starts in `Started` state, during this state `started` method get
/// called.
///
/// ## Running
///
/// After Actor's method `started` get called, actor transitions to `Running`
/// state. Actor can stay in `running` state indefinitely long.
///
/// ## Stopping
///
/// Actor execution state changes to `stopping` state in following situations,
///
/// * `Context::stop` get called by actor itself
/// * all addresses to the actor get dropped
/// * no evented objects are registered in context.
///
/// Actor could restore from `stopping` state to `running` state by creating new
/// address or adding evented object, like future or stream, in
/// `Actor::stopping` method.
///
/// If actor changed state to a `stopping` state because of `Context::stop()`
/// get called then context immediately stops processing incoming messages and
/// calls `Actor::stopping()` method. If actor does not restore back to a
/// `running` state, all unprocessed messages get dropped.
///
/// ## Stopped
///
/// If actor does not modify execution context during stopping state actor
/// state changes to `Stopped`. This state is considered final and at this
/// point actor get dropped.
///
pub trait Actor: Sized + 'static {
    /// Actor execution context type
    type Context: ActorContext;

    /// Method is called when actor get polled first time.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Method is called after an actor is in `Actor::Stopping` state. There
    /// could be several reasons for stopping. `Context::stop` get called
    /// by the actor itself. All addresses to current actor get dropped and
    /// no more evented objects left in the context.
    ///
    /// Actor could restore from stopping state by returning
    /// `Running::Continue` value.
    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        Running::Stop
    }

    /// Method is called after an actor is stopped, it can be used to perform
    /// any needed cleanup work or spawning more actors. This is final state,
    /// after this call actor get dropped.
    fn stopped(&mut self, ctx: &mut Self::Context) {}

    /// Start new asynchronous actor, returns address of newly created actor.
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
    ///         let addr = MyActor.start(); // <- start actor and get it's address
    /// #       System::current().stop();
    ///     });
    /// }
    /// ```
    fn start(self) -> Addr<Self>
    where
        Self: Actor<Context = Context<Self>>,
    {
        let ctx = Context::new(Some(self));
        let addr = ctx.address();
        ctx.run();
        addr
    }

    /// Start new asynchronous actor, returns address of newly created actor.
    fn start_default() -> Addr<Self>
    where
        Self: Actor<Context = Context<Self>> + Default,
    {
        Self::default().start()
    }

    /// Use `create` method, if you need `Context` object during actor
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
        F: FnOnce(&mut Context<Self>) -> Self + 'static,
    {
        let ctx = Context::create(f);
        let addr = ctx.address();

        Arbiter::spawn(ctx);
        addr
    }
}

#[allow(unused_variables)]
/// Actors with ability to restart after failure
///
/// Supervised actors can be managed by [Supervisor](struct.Supervisor.html).
/// Lifecycle events are extended with `restarting` method.
/// If actor fails, supervisor creates new execution context and restarts actor.
/// `restarting` method is called during restart. After call to this method
/// Actor execute state changes to `Started` and normal lifecycle process
/// starts.
///
/// `restarting` method get called with newly constructed `Context` object.
pub trait Supervised: Actor {
    /// Method called when supervisor restarting failed actor
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
    /// Indicates if actor is alive
    pub fn alive(&self) -> bool {
        *self == ActorState::Started || *self == ActorState::Running
    }
    /// Indicates if actor is stopped of stopping
    pub fn stopping(&self) -> bool {
        *self == ActorState::Stopping || *self == ActorState::Stopped
    }
}

/// Actor execution context
///
/// Each actor runs within specific execution context. `Actor::Context` defines
/// context. Execution context defines type of execution, actor communication
/// channels (message handling).
pub trait ActorContext: Sized {
    /// Immediately stop processing incoming messages and switch to a
    /// `stopping` state
    fn stop(&mut self);

    /// Terminate actor execution
    fn terminate(&mut self);

    /// Actor execution state
    fn state(&self) -> ActorState;
}

/// Asynchronous execution context
pub trait AsyncContext<A>: ActorContext
where
    A: Actor<Context = Self>,
{
    /// Return `Address` of the context
    fn address(&self) -> Addr<A>;

    /// Spawn async future into context. Returns handle of the item,
    /// could be used for cancelling execution.
    ///
    /// All futures cancel during actor stopping stage.
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
    where
        F: ActorFuture<Item = (), Error = (), Actor = A> + 'static;

    /// Spawn future into the context. Stop processing any of incoming events
    /// until this future resolves.
    fn wait<F>(&mut self, fut: F)
    where
        F: ActorFuture<Item = (), Error = (), Actor = A> + 'static;

    /// Check if context is paused (waiting for future completion or stopping)
    fn waiting(&self) -> bool;

    /// Cancel future. handle is a value returned by `spawn` method.
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool;

    /// This method register stream to an actor context and
    /// allows to handle `Stream` in similar way as normal actor messages.
    ///
    /// ```rust
    /// # #[macro_use] extern crate actix;
    /// # extern crate futures;
    /// # use std::io;
    /// use actix::prelude::*;
    /// use futures::stream::once;
    ///
    /// #[derive(Message)]
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl StreamHandler<Ping, io::Error> for MyActor {
    ///
    ///     fn handle(&mut self, item: Ping, ctx: &mut Context<MyActor>) {
    ///         println!("PING");
    /// #       System::current().stop();
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
    ///        ctx.add_stream(once::<Ping, io::Error>(Ok(Ping)));
    ///    }
    /// }
    /// # fn main() {
    /// #    let sys = System::new("example");
    /// #    let addr = MyActor.start();
    /// #    sys.run();
    /// # }
    /// ```
    fn add_stream<S>(&mut self, fut: S) -> SpawnHandle
    where
        S: Stream + 'static,
        A: StreamHandler<S::Item, S::Error>,
    {
        <A as StreamHandler<S::Item, S::Error>>::add_stream(fut, self)
    }

    /// This method register stream to an actor context and
    /// allows to handle `Stream` in similar way as normal actor messages.
    ///
    /// ```rust
    /// # #[macro_use] extern crate actix;
    /// # extern crate futures;
    /// # use std::io;
    /// use actix::prelude::*;
    /// use futures::stream::once;
    ///
    /// #[derive(Message)]
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl StreamHandler2<Ping, io::Error> for MyActor {
    ///     fn handle(&mut self, msg: io::Result<Option<Ping>>, ctx: &mut Context<MyActor>) {
    ///         match msg {
    ///             Ok(Some(_)) => println!("PING"),
    ///             _ => println!("finished"),
    ///         }
    /// #       System::current().stop();
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    ///
    ///     fn started(&mut self, ctx: &mut Context<Self>) {
    ///         // add stream
    ///         ctx.add_stream2(once::<Ping, io::Error>(Ok(Ping)));
    ///     }
    /// }
    /// # fn main() {
    /// #    System::run(|| {
    /// #        let addr = MyActor.start();
    /// #    });
    /// # }
    /// ```
    fn add_stream2<S>(&mut self, fut: S) -> SpawnHandle
    where
        S: Stream + 'static,
        A: StreamHandler2<S::Item, S::Error>,
    {
        <A as StreamHandler2<S::Item, S::Error>>::add_stream(fut, self)
    }

    /// This method is similar to `add_stream` but it skips stream errors.
    ///
    /// ```rust
    /// # #[macro_use] extern crate actix;
    /// # extern crate futures;
    /// use actix::prelude::*;
    /// use futures::stream::once;
    ///
    /// #[derive(Message)]
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
    ///         ctx.add_message_stream(once(Ok(Ping)));
    ///     }
    /// }
    /// # fn main() {
    /// #    System::run(|| {
    /// #        let addr = MyActor.start();
    /// #    });
    /// # }
    /// ```
    fn add_message_stream<S>(&mut self, fut: S)
    where
        S: Stream<Error = ()> + 'static,
        S::Item: Message,
        A: Handler<S::Item>,
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_message_stream called for stopped actor.");
        } else {
            self.spawn(ActorMessageStreamItem::new(fut));
        }
    }

    /// Send message `msg` to self.
    fn notify<M>(&mut self, msg: M)
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_timeout called for stopped actor.");
        } else {
            self.spawn(ActorMessageItem::new(msg));
        }
    }

    /// Send message `msg` to self after specified period of time. Returns
    /// spawn handle which could be used for cancellation. Notification get
    /// cancelled if context's stop method get called.
    fn notify_later<M>(&mut self, msg: M, after: Duration) -> SpawnHandle
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_timeout called for stopped actor.");
            SpawnHandle::default()
        } else {
            self.spawn(ActorDelayedMessageItem::new(msg, after))
        }
    }

    /// Execute closure after specified period of time within same Actor and
    /// Context. Execution get cancelled if context's stop method get
    /// called.
    fn run_later<F>(&mut self, dur: Duration, f: F) -> SpawnHandle
    where
        F: FnOnce(&mut A, &mut A::Context) + 'static,
    {
        self.spawn(TimerFunc::new(dur, f))
    }

    ///Spawns job to execute closure with specified interval
    fn run_interval<F>(&mut self, dur: Duration, f: F) -> SpawnHandle
    where
        F: FnMut(&mut A, &mut A::Context) + 'static,
    {
        self.spawn(IntervalFunc::new(dur, f).finish())
    }
}

/// Spawned future handle. Could be used for cancelling spawned future.
#[derive(Eq, PartialEq, Debug, Copy, Clone, Hash)]
pub struct SpawnHandle(usize);

impl SpawnHandle {
    /// Get next handle
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
