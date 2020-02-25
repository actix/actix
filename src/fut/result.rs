//! Definition of the `Result` (immediately finished) combinator
use pin_project::pin_project;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task;
use std::task::Poll;

use crate::actor::Actor;
use crate::fut::ActorFuture;

/// A future representing a value that is immediately ready.
///
/// Created by the `result` function.
#[pin_project]
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
// TODO: rename this to `Result` on the next major version
pub struct FutureResult<T, E, A> {
    inner: Option<Result<T, E>>,
    act: PhantomData<A>,
}

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
/// let future_of_1 = fut::result::<u32, u32, MyActor>(Ok(1));
/// let future_of_err_2 = fut::result::<u32, u32, MyActor>(Err(2));
/// ```
pub fn result<T, E, A>(r: Result<T, E>) -> FutureResult<T, E, A> {
    FutureResult {
        inner: Some(r),
        act: PhantomData,
    }
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
/// let future_of_1 = ok::<u32, u32, MyActor>(1);
/// ```
pub fn ok<T, E, S>(t: T) -> FutureResult<T, E, S> {
    result(Ok(t))
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
/// let future_of_err_1 = fut::err::<u32, u32, MyActor>(1);
/// ```
pub fn err<T, E, A>(e: E) -> FutureResult<T, E, A> {
    result(Err(e))
}

impl<T, E, A> ActorFuture for FutureResult<T, E, A>
where
    A: Actor,
{
    type Output = Result<T, E>;
    type Actor = A;

    fn poll(
        self: Pin<&mut Self>,
        _: &mut Self::Actor,
        _: &mut <Self::Actor as Actor>::Context,
        _: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        Poll::Ready(
            self.project()
                .inner
                .take()
                .expect("cannot poll Result twice"),
        )
    }
}

impl<T, E, A> From<Result<T, E>> for FutureResult<T, E, A> {
    fn from(r: Result<T, E>) -> Self {
        result(r)
    }
}
