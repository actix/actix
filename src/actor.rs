use std;
use std::time::Duration;
use futures::{future, Future, Stream};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::{Encoder, Decoder};

use fut::ActorFuture;
use message::Response;
use arbiter::Arbiter;
use address::ActorAddress;
use envelope::ToEnvelope;
use context::{Context, ActorFutureCell, ActorStreamCell};
use framed::FramedContext;
use utils::{TimerFunc, TimeoutWrapper};


#[allow(unused_variables)]
/// Actors are objects which encapsulate state and behavior.
///
/// Actors run within specific execution context
/// [Context<A>](https://fafhrd91.github.io/actix/actix/struct.Context.html).
/// Context object is available only during execution. Each actor has separate
/// execution context. Also execution context controls lifecycle of an actor.
///
/// Actors communicate exclusively by exchanging messages. Sender actor can
/// wait for response. Actors are not referenced dirrectly, but by
/// non thread safe [Address<A>](https://fafhrd91.github.io/actix/actix/struct.Address.html)
/// or thread safe address
/// [`SyncAddress<A>`](https://fafhrd91.github.io/actix/actix/struct.SyncAddress.html)
/// To be able to handle specific message actor has to provide
/// [`Handler<M>`](
/// file:///Users/nikki/personal/ctx/target/doc/actix/trait.Handler.html)
/// implementation for this message. All messages are statically typed. Message could be
/// handled in asynchronous fasion. Actor can spawn other actors or add futures or
/// streams to execution context. Actor trait provides several method that allows
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
/// Aftre Actor's method `started` get called, actor transitiones to `Running` state.
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
    type Context: ActorContext<Self>;

    /// Method is called when actor get polled first time.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Method is called after an actor is in `Actor::Stopping` state. There could be several
    /// reasons for stopping. `Context::stop` get called by the actor itself.
    /// All addresses to current actor get dropped and no more evented objects
    /// left in the context. Actor could restore from stopping state to running state
    /// by creating new address or adding future or stream to current content.
    fn stopping(&mut self, ctx: &mut Self::Context) {}

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
        let mut ctx = Context::new(self);
        let addr =  <Self as ActorAddress<Self, Addr>>::get(&mut ctx);
        ctx.run(Arbiter::handle());
        addr
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
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()});
        let addr =  <Self as ActorAddress<Self, Addr>>::get(&mut ctx);

        Arbiter::handle().spawn_fn(move || {
            let act = f(&mut ctx);
            let old = ctx.replace_actor(act);
            std::mem::forget(old);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }

    /// Create static response.
    fn reply<M>(val: M::Item) -> Response<Self, M> where M: ResponseType {
        Response::reply(val)
    }

    /// Create async response process.
    fn async_reply<T, M>(fut: T) -> Response<Self, M>
        where M: ResponseType,
              T: ActorFuture<Item=M::Item, Error=M::Error, Actor=Self> + Sized + 'static
    {
        Response::async_reply(fut)
    }

    /// Create unit response, for case when `ResponseType::Item = ()`
    fn empty<M>() -> Response<Self, M> where M: ResponseType<Item=()> {
        Response::empty()
    }

    /// Create error response
    fn reply_error<M>(err: M::Error) -> Response<Self, M> where M: ResponseType {
        Response::error(err)
    }
}

/// Actor trait that allows to handle `tokio_io::codec::Framed` objects.
#[allow(unused_variables)]
pub trait FramedActor: Actor {
    /// Io type
    type Io: AsyncRead + AsyncWrite;
    /// Codec type
    type Codec: Encoder + Decoder;

    /// Method is called on sink error. By default it does nothing.
    fn error(&mut self, err: <Self::Codec as Encoder>::Error, ctx: &mut Self::Context) {}

    /// Start new actor, returns address of this actor.
    fn framed<Addr>(self, io: Self::Io, codec: Self::Codec) -> Addr
        where Self: Actor<Context=FramedContext<Self>> + ActorAddress<Self, Addr>,
              Self: StreamHandler<<<Self as FramedActor>::Codec as Decoder>::Item,
                                  <<Self as FramedActor>::Codec as Decoder>::Error>,
            <<Self as FramedActor>::Codec as Decoder>::Item: ResponseType,
    {
        let mut ctx = FramedContext::new(self, io, codec);
        let addr =  <Self as ActorAddress<Self, Addr>>::get(&mut ctx);
        ctx.run(Arbiter::handle());
        addr
    }

    /// This function starts new actor, returns address of this actor.
    /// Actor is created by factory function.
    fn create_framed<Addr, F>(io: Self::Io, codec: Self::Codec, f: F) -> Addr
        where Self: Actor<Context=FramedContext<Self>> + ActorAddress<Self, Addr>,
              Self: StreamHandler<<<Self as FramedActor>::Codec as Decoder>::Item,
                                  <<Self as FramedActor>::Codec as Decoder>::Error>,
            <<Self as FramedActor>::Codec as Decoder>::Item: ResponseType,
              F: FnOnce(&mut FramedContext<Self>) -> Self + 'static
    {
        let mut ctx = FramedContext::new(unsafe{std::mem::uninitialized()}, io, codec);
        let addr =  <Self as ActorAddress<Self, Addr>>::get(&mut ctx);

        Arbiter::handle().spawn_fn(move || {
            let act = f(&mut ctx);
            let old = ctx.replace_actor(act);
            std::mem::forget(old);
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
/// Livecycle events are extended with `restarting` state for supervised actors.
/// If actor failes supervisor create new execution context and restart actor.
/// `restarting` method is called during restart. After call to this method
/// Actor execute state changes to `Started` and normal lifecycle process starts.
pub trait Supervised: Actor {

    /// Method called when supervisor restarting failed actor
    fn restarting(&mut self, ctx: &mut <Self as Actor>::Context) {}
}

/// Message handler
///
/// `Handler` implementation is a general way how to handle
/// incoming messages, streams, futures.
///
/// `M` is a message which can be handled by the actor.
/// `E` is an optional error type, if message handler is used for handling
/// Future or Stream results, then `E` type has to be set to correspondent `Error` type.
#[allow(unused_variables)]
pub trait Handler<M, E=()> where Self: Actor, M: ResponseType
{
    /// Method is called for every message received by this Actor
    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> Response<Self, M>;

    /// Method is called on error. By default it does nothing.
    fn error(&mut self, err: E, ctx: &mut Self::Context) {}
}

/// Message response type
pub trait ResponseType {

    /// The type of value that this message will resolved with if it is successful.
    type Item;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error;
}

/// Stream handler
///
/// `StreamHandler` is an extension of a `Handler` with stream specific methods.
#[allow(unused_variables)]
pub trait StreamHandler<M, E=()>: Handler<M, E>
    where Self: Actor,
          M: ResponseType,
{
    /// Method is called when stream get polled first time.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Method is called when stream finishes, even if stream finishes with error.
    fn finished(&mut self, ctx: &mut Self::Context) {}
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
/// context. Execution context defines type of execution, actor communition channels
/// (message handling).
pub trait ActorContext<A>: ToEnvelope<A> + Sized where A: Actor<Context=Self> {

    /// Gracefuly stop actor execution
    fn stop(&mut self);

    /// Terminate actor execution
    fn terminate(&mut self);

    /// Actor execution state
    fn state(&self) -> ActorState;

    /// Check if execution context is alive
    fn alive(&self) -> bool {
        self.state() == ActorState::Stopped
    }

    /// Get actor address
    fn address<Address>(&mut self) -> Address where A: ActorAddress<A, Address>
    {
        <A as ActorAddress<A, Address>>::get(self)
    }
}

/// Spawned future handle. Could be used for cancelling spawned future.
#[derive(PartialEq, Debug, Copy, Clone)]
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

/// Asynchronous execution context
pub trait AsyncContext<A>: ActorContext<A> where A: Actor<Context=Self>
{
    /// Spawn async future into context. Returns handle of the item,
    /// could be used for cancelling execution.
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static;

    /// Spawn future into the context. Stop processing any of incoming events
    /// until this future resolves.
    fn wait<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static;

    /// Cancel future. idx is a value returned by `spawn` method.
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool;

    /// This method allow to handle Future in similar way as normal actor messages.
    ///
    /// ```rust
    /// extern crate actix;
    ///
    /// use std::time::Duration;
    /// use actix::prelude::*;
    ///
    /// // Message
    /// struct Ping;
    ///
    /// impl ResponseType for Ping {
    ///     type Item = ();
    ///     type Error = ();
    /// }
    ///
    /// struct MyActor;
    ///
    /// impl Handler<Ping, std::io::Error> for MyActor {
    ///     fn error(&mut self, err: std::io::Error, ctx: &mut Context<MyActor>) {
    ///         println!("Error: {}", err);
    ///     }
    ///     fn handle(&mut self, msg: Ping, ctx: &mut Context<MyActor>) -> Response<Self, Ping> {
    ///         println!("PING");
    ///         Self::empty()
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///    type Context = Context<Self>;
    ///
    ///    fn started(&mut self, ctx: &mut Context<Self>) {
    ///        // send `Ping` to self.
    ///        ctx.notify(Ping, Duration::new(0, 1000));
    ///    }
    /// }
    /// fn main() {}
    /// ```
    fn add_future<F>(&mut self, fut: F)
        where F: Future + 'static,
              F::Item: ResponseType,
              A: Handler<F::Item, F::Error>
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_future called for stopped actor.");
        } else {
            self.spawn(ActorFutureCell::new(fut));
        }
    }

    /// This method is similar to `add_future` but works with streams.
    ///
    /// Information to consider. Actor wont receive next item from a stream
    /// until `Response` future resolves to result. `Self::reply` and
    /// `Self::reply_error` resolves immediately.
    fn add_stream<S>(&mut self, fut: S)
        where S: Stream + 'static,
              S::Item: ResponseType,
              A: Handler<S::Item, S::Error> + StreamHandler<S::Item, S::Error>
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_stream called for stopped actor.");
        } else {
            self.spawn(ActorStreamCell::new(fut));
        }
    }

    /// Send message `msg` to self after specified period of time. Returns spawn handle
    /// which could be used for cancelation.
    fn notify<M, E>(&mut self, msg: M, after: Duration) -> SpawnHandle
        where A: Handler<M, E>, M: ResponseType + 'static, E: 'static
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_timeout called for stopped actor.");
            SpawnHandle::default()
        } else {
            self.spawn(
                ActorFutureCell::new(TimeoutWrapper::new(msg, after)))
        }
    }

    /// Execute closure after specified period of time within same Actor and Context
    fn run_later<F>(&mut self, dur: Duration, f: F) -> SpawnHandle
        where F: FnOnce(&mut A, &mut A::Context) + 'static
    {
        self.spawn(TimerFunc::new(dur, f))
    }
}
