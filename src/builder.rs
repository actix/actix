//! Definition of the `ActorBuilder` trait and implementation
//!
use std;
use futures::{future, Stream};

use actor::{Actor, MessageHandler, StreamHandler};
use address::ActorAddress;
use arbiter::Arbiter;
use context::Context;

/// Builds an actor
///
/// All `Actor` types implements `ActorBuilder` trait. `ActorBuilder` has to
/// be used for actor creation and running. Builder start actor in current `Arbiter`
/// (i.e `Arbiter::handle()`). All trait's methods can return different types of
/// address (address type needs to be infered)
///
/// # Examples
///
/// ```rust
/// use actix::*;
///
/// // initialize system
/// System::new("test".to_owned());
///
/// struct MyActor;
/// impl Actor for MyActor {}
///
/// let addr: Address<MyActor> = MyActor.start();
/// ```
///
/// If address of newly created actor is not required, explicit `()` annotation needs
/// to be use (some discussion on this topic is in #27336)
///
/// # Examples
///
/// ```rust
/// use actix::*;
///
/// struct MyActor;
/// impl Actor for MyActor {}
///
/// fn main() {
///    // initialize system
///    let sys = System::new("test".to_owned());
///
///    let _: () = MyActor.start();
/// }
/// ```
///
/// It is possible to start actor and connect to stream.
/// `StreamHandler` trait has to be implemented for this actor.
///
/// # Examples
///
/// ```rust
/// extern crate actix;
/// extern crate futures;
///
/// use futures::Stream;
/// use actix::prelude::*;
///
/// struct MyActor;
/// impl Actor for MyActor {}
///
/// struct Message;
///
/// impl StreamHandler<Message, ()> for MyActor {}
///
/// impl MessageResponse<Message> for MyActor {
///     type Item = ();
///     type Error = ();
/// }
///
/// impl MessageHandler<Message> for MyActor {
///    fn handle(&mut self, msg: Message, ctx: &mut Context<Self>)
///              -> Response<Self, Message> {
///        ().to_response()
///    }
/// }
///
/// fn start<S>(stream: S)
///      where S: futures::Stream<Item=Message, Error=()> + 'static
/// {
///     let _: () = MyActor.start_with(stream);
/// }
///
/// fn main() {}
/// ```
///
/// If for actor initialization execution context is required then
/// `create` and `create_with` method should be used. Both methods are equivalent to
/// `start` and `start_with` methods except both accept closure which has to return
/// instance of actor.
pub trait ActorBuilder<A, Addr=()>
    where A: Actor + Sized + 'static,
          Self: ActorAddress<A, Addr>,
{
    /// Start new actor, returns address of newly created actor.
    /// This is special method if Actor implement `Default` trait.
    fn run() -> Addr where Self: Default;

    /// Start new actor, returns address of newly created actor.
    fn start(self) -> Addr;

    /// Start actor and register stream
    fn start_with<S>(self, stream: S) -> Addr
        where S: Stream + 'static,
              S::Item: 'static,
              A: MessageHandler<S::Item, S::Error> + StreamHandler<S::Item, S::Error>;

    fn create<F>(f: F) -> Addr
        where F: FnOnce(&mut Context<A>) -> A + 'static;

    fn create_with<S, F>(stream: S, f: F) -> Addr
        where F: FnOnce(&mut Context<A>) -> A + 'static,
              S: Stream + 'static,
              A: MessageHandler<S::Item, S::Error> + StreamHandler<S::Item, S::Error>;
}

impl<A, Addr> ActorBuilder<A, Addr> for A
    where A: Actor,
          Self: ActorAddress<A, Addr>,
{
    fn run() -> Addr where Self: Default
    {
        let mut ctx = Context::new(Self::default());
        let addr =  <Self as ActorAddress<A, Addr>>::get(&mut ctx);
        ctx.run(Arbiter::handle());
        addr
    }

    fn start(self) -> Addr
    {
        let mut ctx = Context::new(self);
        let addr =  <Self as ActorAddress<A, Addr>>::get(&mut ctx);
        ctx.run(Arbiter::handle());
        addr
    }

    fn start_with<S>(self, stream: S) -> Addr
        where S: Stream + 'static,
              S::Item: 'static,
              A: MessageHandler<S::Item, S::Error> + StreamHandler<S::Item, S::Error>,
    {
        let mut ctx = Context::new(self);
        ctx.add_stream(stream);
        let addr =  <Self as ActorAddress<A, Addr>>::get(&mut ctx);
        ctx.run(Arbiter::handle());
        addr
    }

    fn create<F>(f: F) -> Addr
        where F: 'static + FnOnce(&mut Context<A>) -> A,
    {
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()});
        let addr =  <Self as ActorAddress<A, Addr>>::get(&mut ctx);

        Arbiter::handle().spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_actor(srv);
            std::mem::forget(old);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }

    fn create_with<S, F: 'static>(stream: S, f: F) -> Addr
        where F: 'static + FnOnce(&mut Context<A>) -> A,
              S: Stream + 'static,
              S::Item: 'static,
              S::Error: 'static,
              A: MessageHandler<S::Item, S::Error> + StreamHandler<S::Item, S::Error>,
    {
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()});
        let addr =  <Self as ActorAddress<A, Addr>>::get(&mut ctx);

        Arbiter::handle().spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_actor(srv);
            std::mem::forget(old);
            ctx.add_stream(stream);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }
}

