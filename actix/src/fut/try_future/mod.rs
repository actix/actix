use std::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::{actor::Actor, fut::future::ActorFuture};

mod and_then;
mod map_err;
mod map_ok;

pub use and_then::AndThen;
pub use map_err::MapErr;
pub use map_ok::MapOk;

mod private_try_act_future {
    use super::{Actor, ActorFuture};

    pub trait Sealed<A> {}

    impl<A, F, T, E> Sealed<A> for F
    where
        A: Actor,
        F: ?Sized + ActorFuture<A, Output = Result<T, E>>,
    {
    }
}

/// A convenience for actor futures that return `Result` values that includes
/// a variety of adapters tailored to such actor futures.
pub trait ActorTryFuture<A: Actor>: ActorFuture<A> + private_try_act_future::Sealed<A> {
    /// The type of successful values yielded by this actor future
    type Ok;

    /// The type of failures yielded by this actor  future
    type Error;

    /// Poll this `ActorTryFuture` as if it were a `ActorFuture`.
    ///
    /// This method is a stopgap for a compiler limitation that prevents us from
    /// directly inheriting from the `ActorFuture` trait; in the actor future it
    /// won't be needed.
    fn try_poll(
        self: Pin<&mut Self>,
        srv: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Result<Self::Ok, Self::Error>>;
}

impl<A, F, T, E> ActorTryFuture<A> for F
where
    A: Actor,
    F: ActorFuture<A, Output = Result<T, E>> + ?Sized,
{
    type Ok = T;
    type Error = E;

    fn try_poll(
        self: Pin<&mut Self>,
        srv: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<F::Output> {
        self.poll(srv, ctx, task)
    }
}

/// Adapters specific to [`Result`]-returning actor futures
pub trait ActorTryFutureExt<A: Actor>: ActorTryFuture<A> {
    /// Executes another actor future after this one resolves successfully.
    /// The success value is passed to a closure to create this subsequent
    /// actor future.
    ///
    /// The provided closure `f` will only be called if this actor future is
    /// resolved to an [`Ok`]. If this actor future resolves to an  [`Err`],
    /// panics, or is dropped, then the provided closure will never
    /// be invoked. The [`Error`](ActorTryFuture::Error) type of
    /// this actor future and the actor future returned by `f` have to match.
    ///
    /// Note that this method consumes the actor future it is called on and
    /// returns a wrapped version of it.
    fn and_then<Fut, F>(self, f: F) -> AndThen<Self, Fut, F>
    where
        F: FnOnce(Self::Ok, &mut A, &mut A::Context) -> Fut,
        Fut: ActorTryFuture<A, Error = Self::Error>,
        Self: Sized,
    {
        and_then::new(self, f)
    }

    /// Maps this actor future's success value to a different value.
    ///
    /// This method can be used to change the [`Ok`](ActorTryFuture::Ok) type
    /// of the actor future into a different type. It is similar to the
    /// [`Result::map`]  method. You can use this method to chain along a
    /// computation once the actor future has been resolved.
    ///
    /// The provided closure `f` will only be called if this actor future is
    /// resolved to an [`Ok`]. If it resolves to an [`Err`], panics, or is
    /// dropped, then the provided closure will never be invoked.
    ///
    /// Note that this method consumes the actor future it is called on and
    /// returns a wrapped version of it.
    fn map_ok<T, F>(self, f: F) -> MapOk<Self, F>
    where
        F: FnOnce(Self::Ok, &mut A, &mut A::Context) -> T,
        Self: Sized,
    {
        MapOk::new(self, f)
    }

    /// Maps this actor future's error value to a different value.
    ///
    /// This method can be used to change the [`Error`](ActorTryFuture::Error)
    /// type of the actor future into a different type. It is similar to the
    /// [`Result::map_err`] method.
    ///
    /// The provided closure `f` will only be called if this actor future is
    /// resolved to an [`Err`]. If it resolves to an [`Ok`], panics, or is
    /// dropped, then the provided closure will never be invoked.
    ///
    /// Note that this method consumes the actor future it is called on and
    /// returns a wrapped version of it.
    fn map_err<T, F>(self, f: F) -> MapErr<Self, F>
    where
        F: FnOnce(Self::Error, &mut A, &mut A::Context) -> T,
        Self: Sized,
    {
        MapErr::new(self, f)
    }
}

impl<A, F> ActorTryFutureExt<A> for F
where
    A: Actor,
    F: ActorTryFuture<A> + ?Sized,
{
}
