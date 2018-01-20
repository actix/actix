use std::time::Duration;
use futures::{future, Future, Stream};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::{Framed, Encoder, Decoder};

use fut::ActorFuture;
use message::Response;
use arbiter::Arbiter;
use address::ActorAddress;
use envelope::ToEnvelope;
use handler::{Handler, ResponseType};
use context::Context;
use contextitems::{ActorFutureItem, ActorMessageItem,
                   ActorDelayedMessageItem, ActorStreamItem, ActorMessageStreamItem};
use framed::FramedContext;
use utils::TimerFunc;


#[allow(unused_variables)]
/// Actors are objects which encapsulate state and behavior.
///
/// Actors run within specific execution context
/// [Context<A>](https://fafhrd91.github.io/actix/actix/struct.Context.html).
/// Context object is available only during execution. Each actor has separate
/// execution context. Also execution context controls lifecycle of an actor.
///
/// Actors communicate exclusively by exchanging messages. Sender actor can
/// wait for response. Actors are not referenced directly, but by
/// non thread safe [Address<A>](https://fafhrd91.github.io/actix/actix/struct.Address.html)
/// or thread safe address
/// [`SyncAddress<A>`](https://fafhrd91.github.io/actix/actix/struct.SyncAddress.html)
/// To be able to handle specific message actor has to provide
/// [`Handler<M>`](
/// file:///Users/nikki/personal/ctx/target/doc/actix/trait.Handler.html)
/// implementation for this message. All messages are statically typed. Message could be
/// handled in asynchronous fashion. Actor can spawn other actors or add futures or
/// streams to execution context. Actor trait provides several methods that allow
/// to control actor lifecycle.
///
/// # Actor lifecycle
///
/// ## Started
///
/// Actor starts in `Started` state, during this state `started` method get called.
///
/// ## Running
///
/// After Actor's method `started` get called, actor transitions to `Running` state.
/// Actor can stay in `running` state indefinitely long.
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
/// address or adding evented object, like future or stream, in `Actor::stopping` method.
///
/// ## Stopped
///
/// If actor does not modify execution context during stooping state actor state changes
/// to `Stopped`. This state is considered final and at this point actor get dropped.
///
pub trait Actor: Sized + 'static {

    /// Actor execution context type
    type Context: ActorContext + ToEnvelope<Self>;

    /// Method is called when actor get polled first time.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Method is called after an actor is in `Actor::Stopping` state. There could be several
    /// reasons for stopping. `Context::stop` get called by the actor itself.
    /// All addresses to current actor get dropped and no more evented objects
    /// left in the context.
    ///
    /// Actor could restore from stopping state by returning `false` value.
    fn stopping(&mut self, ctx: &mut Self::Context) -> bool {
        true
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
    /// // initialize system
    /// System::new("test");
    ///
    /// struct MyActor;
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    /// }
    ///
    /// let addr: Address<_> = MyActor.start();
    /// ```
    fn start<Addr>(self) -> Addr
        where Self: Actor<Context=Context<Self>> + ActorAddress<Self, Addr>
    {
        let mut ctx = Context::new(Some(self));
        let addr =  <Self as ActorAddress<Self, Addr>>::get(&mut ctx);
        ctx.run(Arbiter::handle());
        addr
    }

    /// Start new asynchronous actor, returns address of newly created actor.
    fn start_default<Addr>() -> Addr
        where Self: Default + Actor<Context=Context<Self>> + ActorAddress<Self, Addr>
    {
        Self::default().start()
    }

    /// Use `create` method, if you need `Context` object during actor initialization.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use actix::*;
    ///
    /// // initialize system
    /// System::new("test");
    ///
    /// struct MyActor{val: usize};
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    /// }
    ///
    /// let addr: Address<_> = MyActor::create(|ctx: &mut Context<MyActor>| {
    ///     MyActor{val: 10}
    /// });
    /// ```
    fn create<Addr, F>(f: F) -> Addr
        where Self: Actor<Context=Context<Self>> + ActorAddress<Self, Addr>,
              F: FnOnce(&mut Context<Self>) -> Self + 'static
    {
        let mut ctx = Context::new(None);
        let addr =  <Self as ActorAddress<Self, Addr>>::get(&mut ctx);

        Arbiter::handle().spawn_fn(move || {
            let act = f(&mut ctx);
            ctx.set_actor(act);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }

    /// Create static response.
    fn reply<M>(val: Result<M::Item, M::Error>) -> Response<Self, M> where M: ResponseType {
        Response::reply(val)
    }

    /// Create async response process.
    fn async_reply<T, M>(fut: T) -> Response<Self, M>
        where M: ResponseType,
              T: ActorFuture<Item=M::Item, Error=M::Error, Actor=Self> + Sized + 'static {
        Response::async_reply(fut)
    }
}

/// Actor trait that allows to handle `tokio_io::codec::Framed` objects.
#[allow(unused_variables)]
pub trait FramedActor: Actor {
    /// Io type
    type Io: AsyncRead + AsyncWrite;
    /// Codec type
    type Codec: Encoder + Decoder;

    /// This message is called for every decoded message from framed object.
    fn handle(&mut self,
              msg: Result<<Self::Codec as Decoder>::Item, <Self::Codec as Decoder>::Error>,
              ctx: &mut Self::Context);

    /// This method is called when framed object get closed.
    ///
    /// `error` indicates if framed get closed because of error.
    fn closed(&mut self,
              error: Option<<Self::Codec as Encoder>::Error>,
              ctx: &mut Self::Context) {}

    /// Start new actor, returns address of this actor.
    fn framed<Addr>(self, io: Self::Io, codec: Self::Codec) -> Addr
        where Self: Actor<Context=FramedContext<Self>> + ActorAddress<Self, Addr>
    {
        Self::from_framed(self, io.framed(codec))
    }

    /// Start new actor, returns address of this actor.
    fn from_framed<Addr>(self, framed: Framed<Self::Io, Self::Codec>) -> Addr
        where Self: Actor<Context=FramedContext<Self>> + ActorAddress<Self, Addr>
    {
        let mut ctx = FramedContext::framed(Some(self), framed);
        let addr =  <Self as ActorAddress<Self, Addr>>::get(&mut ctx);
        ctx.run(Arbiter::handle());
        addr
    }

    /// This function starts new actor, returns address of this actor.
    /// Actor is created by factory function.
    fn create_framed<Addr, F>(io: Self::Io, codec: Self::Codec, f: F) -> Addr
        where Self: Actor<Context=FramedContext<Self>> + ActorAddress<Self, Addr>,
              F: FnOnce(&mut FramedContext<Self>) -> Self + 'static
    {
        Self::create_from_framed(io.framed(codec), f)
    }

    /// This function starts new actor, returns address of this actor.
    /// Actor is created by factory function.
    fn create_from_framed<Addr, F>(framed: Framed<Self::Io, Self::Codec>, f: F) -> Addr
        where Self: Actor<Context=FramedContext<Self>> + ActorAddress<Self, Addr>,
              F: FnOnce(&mut FramedContext<Self>) -> Self + 'static
    {
        let mut ctx = FramedContext::framed(None, framed);
        let addr =  <Self as ActorAddress<Self, Addr>>::get(&mut ctx);

        Arbiter::handle().spawn_fn(move || {
            let act = f(&mut ctx);
            ctx.set_actor(act);
            ctx.run(Arbiter::handle());
            future::ok(())
        });

        addr
    }
}

#[allow(unused_variables)]
/// Actors with ability to restart after failure
///
/// Supervised actors can be managed by
/// [Supervisor](https://fafhrd91.github.io/actix/actix/struct.Supervisor.html)
/// Lifecycle events are extended with `restarting` state for supervised actors.
/// If actor fails supervisor creates new execution context and restarts actor.
/// `restarting` method is called during restart. After call to this method
/// Actor execute state changes to `Started` and normal lifecycle process starts.
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

/// Actor execution context
///
/// Each actor runs within specific execution context. `Actor::Context` defines
/// context. Execution context defines type of execution, actor communication channels
/// (message handling).
pub trait ActorContext: Sized {

    /// Gracefully stop actor execution
    fn stop(&mut self);

    /// Terminate actor execution
    fn terminate(&mut self);

    /// Actor execution state
    fn state(&self) -> ActorState;

    /// Check if execution context is alive
    fn alive(&self) -> bool {
        self.state() == ActorState::Stopped
    }
}

/// Asynchronous execution context
pub trait AsyncContext<A>: ActorContext + ToEnvelope<A> where A: Actor<Context=Self>
{
    /// Get actor address
    fn address<Address>(&mut self) -> Address where A: ActorAddress<A, Address> {
        <A as ActorAddress<A, Address>>::get(self)
    }

    /// Spawn async future into context. Returns handle of the item,
    /// could be used for cancelling execution.
    ///
    /// All futures cancel during actor stopping stage.
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static;

    /// Spawn future into the context. Stop processing any of incoming events
    /// until this future resolves.
    fn wait<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static;

    /// Check if context is waiting for future completion
    #[doc(hidden)]
    #[inline]
    fn waiting(&self) -> bool { false }

    /// Cancel future. idx is a value returned by `spawn` method.
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool;

    /// This method allow to handle Future in similar way as normal actor messages.
    ///
    /// ```rust
    /// # #[macro_use] extern crate actix;
    /// # extern crate futures;
    /// use std::io;
    /// use actix::prelude::*;
    /// use futures::future;
    ///
    /// #[derive(Message)]
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl Handler<Result<Ping, io::Error>> for MyActor {
    ///     type Result = ();
    ///
    ///     fn handle(&mut self, msg: Result<Ping, io::Error>, ctx: &mut Context<MyActor>) {
    ///         println!("PING");
    /// #       Arbiter::system().send(actix::msgs::SystemExit(0));
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///    type Context = Context<Self>;
    ///
    ///    fn started(&mut self, ctx: &mut Context<Self>) {
    ///        // send `Ping` to self.
    ///        ctx.add_future(future::result::<Ping, io::Error>(Ok(Ping)));
    ///    }
    /// }
    /// # fn main() {
    /// #    let sys = System::new("example");
    /// #    let addr: Address<_> = MyActor.start();
    /// #    sys.run();
    /// # }
    /// ```
    fn add_future<F>(&mut self, fut: F)
        where F: Future + 'static,
              F::Item: ResponseType,
              A: Handler<Result<F::Item, F::Error>>
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_future called for stopped actor.");
        } else {
            self.spawn(ActorFutureItem::new(fut));
        }
    }

    /// This method is similar to `add_future` but works with streams.
    ///
    /// Information to consider. Actor wont receive next item from a stream
    /// until `Response` future resolves to a result. `Self::reply` resolves immediately.
    ///
    /// This method is similar to `add_stream` but it skips result error.
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
    /// impl Handler<Result<Ping, io::Error>> for MyActor {
    ///     type Result = ();
    ///
    ///     fn handle(&mut self, msg: Result<Ping, io::Error>, ctx: &mut Context<MyActor>) {
    ///         println!("PING");
    /// #       Arbiter::system().send(actix::msgs::SystemExit(0));
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
    /// #    let addr: Address<_> = MyActor.start();
    /// #    sys.run();
    /// # }
    /// ```
    fn add_stream<S>(&mut self, fut: S)
        where S: Stream + 'static,
              S::Item: ResponseType,
              A: Handler<Result<S::Item, S::Error>>
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_stream called for stopped actor.");
        } else {
            self.spawn(ActorStreamItem::new(fut));
        }
    }

    /// This method is similar to `add_stream` but it skips result error.
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
    /// #       Arbiter::system().send(actix::msgs::SystemExit(0));
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///    type Context = Context<Self>;
    ///
    ///    fn started(&mut self, ctx: &mut Context<Self>) {
    ///        // add messages stream
    ///        ctx.add_message_stream(once(Ok(Ping)));
    ///    }
    /// }
    /// # fn main() {
    /// #    let sys = System::new("example");
    /// #    let addr: Address<_> = MyActor.start();
    /// #    sys.run();
    /// # }
    /// ```
    fn add_message_stream<S>(&mut self, fut: S)
        where S: Stream<Error=()> + 'static,
              S::Item: ResponseType,
              A: Handler<S::Item>
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_message_stream called for stopped actor.");
        } else {
            self.spawn(ActorMessageStreamItem::new(fut));
        }
    }

    /// Send message `msg` to self.
    fn notify<M>(&mut self, msg: M)
        where A: Handler<M>, M: ResponseType + 'static
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_timeout called for stopped actor.");
        } else {
            self.spawn(ActorMessageItem::new(msg));
        }
    }

    /// Send message `msg` to self after specified period of time. Returns spawn handle
    /// which could be used for cancellation. Notification get cancelled
    /// if context's stop method get called.
    fn notify_later<M>(&mut self, msg: M, after: Duration) -> SpawnHandle
        where A: Handler<M>, M: ResponseType + 'static
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_timeout called for stopped actor.");
            SpawnHandle::default()
        } else {
            self.spawn(ActorDelayedMessageItem::new(msg, after))
        }
    }

    /// Execute closure after specified period of time within same Actor and Context.
    /// Execution get cancelled if context's stop method get called.
    fn run_later<F>(&mut self, dur: Duration, f: F) -> SpawnHandle
        where F: FnOnce(&mut A, &mut A::Context) + 'static
    {
        self.spawn(TimerFunc::new(dur, f))
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
}

impl Default for SpawnHandle {
    fn default() -> SpawnHandle {
        SpawnHandle(0)
    }
}
