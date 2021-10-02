//! Definition of the [`Ready`] (immediately finished) combinator

use std::{
    future::Future,
    pin::Pin,
    task::{self, Poll},
};

use crate::{actor::Actor, fut::ActorFuture};

// TODO: remove re-export and encourage direct usage of std and/or futures crate types.
pub use futures_util::future::{ready, Ready};

/// Creates a new "leaf future" which will resolve with the given result.
///
/// The returned future represents a computation which is finished immediately.
/// This can be useful with the `finished` and `failed` base future types to
/// convert an immediate value to a future to interoperate elsewhere.
///
/// # Examples
///
/// ```
/// use actix::{fut, Actor, Context};
///
/// struct MyActor;
/// impl Actor for MyActor {
///     type Context = Context<Self>;
/// }
///
/// let future_of_1 = fut::result::<u32, u32>(Ok(1));
/// let future_of_err_2 = fut::result::<u32, u32>(Err(2));
/// ```
pub fn result<T, E>(r: Result<T, E>) -> Ready<Result<T, E>> {
    ready(r)
}

/// Creates a "leaf future" from an immediate value of a finished and
/// successful computation.
///
/// The returned future is similar to `result` where it will immediately run a
/// scheduled callback with the provided value.
///
/// # Examples
///
/// ```
/// use actix::fut::*;
/// use actix::{Actor, Context};
///
/// struct MyActor;
/// impl Actor for MyActor {
///     type Context = Context<Self>;
/// }
///
/// let future_of_1 = ok::<u32, u32>(1);
/// ```
pub fn ok<T, E>(t: T) -> Ready<Result<T, E>> {
    ready(Ok(t))
}

/// Creates a "leaf future" from an immediate value of a failed computation.
///
/// The returned future is similar to `result` where it will immediately run a
/// scheduled callback with the provided value.
///
/// # Examples
///
/// ```
/// use actix::{fut, Actor, Context};
///
/// struct MyActor;
/// impl Actor for MyActor {
///     type Context = Context<Self>;
/// }
///
/// let future_of_err_1 = fut::err::<u32, u32>(1);
/// ```
pub fn err<T, E>(e: E) -> Ready<Result<T, E>> {
    ready(Err(e))
}

impl<T, A> ActorFuture<A> for Ready<T>
where
    A: Actor,
{
    type Output = T;

    #[inline]
    fn poll(
        self: Pin<&mut Self>,
        _: &mut A,
        _: &mut A::Context,
        cx: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        Future::poll(self, cx)
    }
}
