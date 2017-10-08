use std;
use futures::{future, Future, Stream};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::{Encoder, Decoder};

use fut::ActorFuture;
use message::Response;
use arbiter::Arbiter;
use address::ActorAddress;
use context::{Context, ActorFutureCell, ActorStreamCell};
use framed::FramedContext;


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

    /// Method is called after an actor is in STOPPING state. There could be several
    /// reasons for stopping. Context::stop get called by actor itself.
    /// All addresses to current actor get dropped and no more evented objects
    /// left in context. Actor could restore from stopping state to running state
    /// by creating new address or adding future or stream to current content.
    fn stopping(&mut self, ctx: &mut Self::Context) {}

    /// Method is called after an actor is stopped, it can be used to perform
    /// any needed cleanup work or spawning more actors.
    fn stopped(&mut self, ctx: &mut Self::Context) {}

    /// Start new asynchronous actor, returns address of this actor.
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
    /// let addr: Address<MyActor> = MyActor.start();
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
    /// let addr: Address<MyActor> = MyActor::create(|ctx| {
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
            let srv = f(&mut ctx);
            let old = ctx.replace_actor(srv);
            std::mem::forget(old);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }

    /// Create response
    fn reply<M>(val: Self::Item) -> Response<Self, M> where Self: ResponseType<M> {
        Response::reply(val)
    }

    /// Create async response
    fn async_reply<T, M>(fut: T) -> Response<Self, M>
        where Self: ResponseType<M>,
              T: ActorFuture<Item=Self::Item, Error=Self::Error, Actor=Self> + Sized + 'static
    {
        Response::async_reply(fut)
    }

    /// Create unit response
    fn empty<M>() -> Response<Self, M> where Self: ResponseType<M, Item=()> {
        Response::empty()
    }

    /// Create error response
    fn reply_error<M>(err: Self::Error) -> Response<Self, M> where Self: ResponseType<M> {
        Response::error(err)
    }
}

/// Actor trait that allow to handle `tokio_io::codec::Framed` objects.
#[allow(unused_variables)]
pub trait FramedActor: Actor {
    type Io: AsyncRead + AsyncWrite;
    type Codec: Encoder + Decoder;

    /// Method is called on sink error. By default it does nothing.
    fn error(&mut self, err: <Self::Codec as Encoder>::Error, ctx: &mut Self::Context) {}

    /// Start new actor, returns address of this actor.
    fn framed<Addr>(self, io: Self::Io, codec: Self::Codec) -> Addr
        where Self: Actor<Context=FramedContext<Self>> + ActorAddress<Self, Addr>,
              Self: StreamHandler<<<Self as FramedActor>::Codec as Decoder>::Item,
                                  <<Self as FramedActor>::Codec as Decoder>::Error>,
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
/// `M` is message which can be handled by actor
/// `E` optional error type, if message handler is used for handling messages
///  from Future or Stream, then `E` type has to be set to correspondent `Error` type.
#[allow(unused_variables)]
pub trait Handler<M, E=()> where Self: Actor + ResponseType<M>
{
    /// Method is called for every message received by this Actor
    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> Response<Self, M>;

    /// Method is called on error. By default it does nothing.
    fn error(&mut self, err: E, ctx: &mut Self::Context) {}
}

/// Response type
pub trait ResponseType<M> where Self: Actor {

    /// The type of value that this message will resolved with if it is successful.
    type Item;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error;
}

/// Stream handler
///
/// `StreamHandler` is an extension of a `Handler` with several stream specific
/// methods.
#[allow(unused_variables)]
pub trait StreamHandler<M, E=()>: Handler<M, E> + ResponseType<M>
    where Self: Actor
{
    /// Method is called when stream get polled first time.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Method is called when stream finishes.
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
pub trait ActorContext<A>: Sized where A: Actor<Context=Self> {

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

/// Asynchronous execution context
pub trait AsyncActorContext<A>: ActorContext<A> where A: Actor<Context=Self>
{
    /// Spawn async future into context
    fn spawn<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static;

    /// This method allow to handle Future in similar way as normal actor message.
    ///
    /// ```rust
    /// extern crate actix;
    /// extern crate futures;
    /// extern crate tokio_core;
    ///
    /// use std::time::Duration;
    /// use futures::Future;
    /// use tokio_core::reactor::Timeout;
    /// use actix::prelude::*;
    ///
    /// // Message
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl ResponseType<Ping> for MyActor {
    ///     type Item = ();
    ///     type Error = ();
    /// }
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
    ///        ctx.add_future(
    ///            Timeout::new(Duration::new(0, 1000), Arbiter::handle()).unwrap()
    ///                .map(|_| Ping)
    ///        );
    ///    }
    /// }
    /// fn main() {}
    /// ```
    fn add_future<F>(&mut self, fut: F)
        where F: Future + 'static, A: Handler<F::Item, F::Error>
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_future called for stopped actor.");
        } else {
            self.spawn(ActorFutureCell::new(fut))
        }
    }

    /// This method is similar to `add_future` but works with streams.
    ///
    /// One note to consider. Actor wont receive next item from a stream
    /// until `Response` future resolves to result. `Self::reply` and
    /// `Self::reply_error` resolves immediately.
    fn add_stream<S>(&mut self, fut: S)
        where S: Stream + 'static,
              A: Handler<S::Item, S::Error> + StreamHandler<S::Item, S::Error>
    {
        if self.state() == ActorState::Stopped {
            error!("Context::add_stream called for stopped actor.");
        } else {
            self.spawn(ActorStreamCell::new(fut))
        }
    }
}
