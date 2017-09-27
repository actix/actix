use std::marker::PhantomData;
use futures::{future, Poll};

mod chain;
mod and_then;
mod result;
mod then;
mod map;
mod map_err;

pub use self::and_then::AndThen;
pub use self::then::Then;
pub use self::map::Map;
pub use self::map_err::MapErr;
pub use self::result::{result, ok, err, FutureResult};

use actor::Actor;
use context::Context;


pub trait ActorFuture {

    /// The type of value that this future will resolved with if it is
    /// successful.
    type Item;

    /// The type of error that this future will resolve with if it fails in a
    /// normal fashion.
    type Error;

    /// The actor within which this future runs
    type Actor: Actor;

    fn poll(&mut self, srv: &mut Self::Actor, ctx: &mut Context<Self::Actor>)
            -> Poll<Self::Item, Self::Error>;

    fn map<F, U>(self, f: F) -> Map<Self, F>
        where F: FnOnce(Self::Item, &mut Self::Actor, &mut Context<Self::Actor>) -> U,
              Self: Sized,
    {
        map::new(self, f)
    }

    fn map_err<F, E>(self, f: F) -> MapErr<Self, F>
        where F: FnOnce(Self::Error, &mut Self::Actor, &mut Context<Self::Actor>) -> E,
              Self: Sized,
    {
        map_err::new(self, f)
    }

    fn then<F, B>(self, f: F) -> Then<Self, B, F>
        where F: FnOnce(Result<Self::Item, Self::Error>,
                        &mut Self::Actor, &mut Context<Self::Actor>) -> B,
              B: IntoActorFuture<Actor=Self::Actor>,
              Self: Sized,
    {
        then::new(self, f)
    }

    /// Execute another future after this one has resolved successfully.
    fn and_then<F, B>(self, f: F) -> AndThen<Self, B, F>
        where F: FnOnce(Self::Item, &mut Self::Actor, &mut Context<Self::Actor>) -> B,
              B: IntoActorFuture<Error=Self::Error, Actor=Self::Actor>,
              Self: Sized,
    {
        and_then::new(self, f)
    }

}


/// Class of types which can be converted into an actor future.
///
/// This trait is very similar to the `IntoIterator` trait and is intended to be
/// used in a very similar fashion.
pub trait IntoActorFuture {
    /// The future that this type can be converted into.
    type Future: ActorFuture<Item=Self::Item, Error=Self::Error, Actor=Self::Actor>;

    /// The item that the future may resolve with.
    type Item;
    /// The error that the future may resolve with.
    type Error;
    /// The actor within which this future runs
    type Actor: Actor;

    /// Consumes this object and produces a future.
    fn into_future(self) -> Self::Future;
}

impl<F: ActorFuture> IntoActorFuture for F {
    type Future = F;
    type Item = F::Item;
    type Error = F::Error;
    type Actor = F::Actor;

    fn into_future(self) -> F {
        self
    }
}

pub trait WrapFuture<A> where A: Actor {
    /// The future that this type can be converted into.
    type Future: ActorFuture<Item=Self::Item, Error=Self::Error, Actor=A>;

    /// The item that the future may resolve with.
    type Item;
    /// The error that the future may resolve with.
    type Error;

    fn actfuture(self) -> Self::Future;
}

impl<F: future::Future, A: Actor> WrapFuture<A> for F {
    type Future = FutureWrap<F, A>;
    type Item = F::Item;
    type Error = F::Error;

    fn actfuture(self) -> Self::Future {
        wrap_future(self)
    }
}

pub struct FutureWrap<F, A> where F: future::Future {
    fut: F,
    act: PhantomData<A>,
}

pub fn wrap_future<F, A>(f: F) -> FutureWrap<F, A>
    where F: future::Future
{
    FutureWrap{fut: f, act: PhantomData}
}

impl<F, A> ActorFuture for FutureWrap<F, A>
    where F: future::Future,
          A: Actor,
{
    type Item = F::Item;
    type Error = F::Error;
    type Actor = A;

    fn poll(&mut self, _: &mut Self::Actor, _: &mut Context<Self::Actor>)
            -> Poll<Self::Item, Self::Error>
    {
        self.fut.poll()
    }
}
