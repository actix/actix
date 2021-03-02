//! Definition of the `Result` (immediately finished) combinator

use super::ready_fut::{ready, Ready};

/// A future representing a value that is immediately ready.
///
/// Created by the `result` function.
#[must_use = "futures do nothing unless polled"]
// TODO: rename this to `Result` on the next major version
// TODO: use std::future::Ready when MSRV surpass 1.48
pub type FutureResult<T, E> = Ready<Result<T, E>>;

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
pub fn result<T, E>(r: Result<T, E>) -> FutureResult<T, E> {
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
pub fn ok<T, E>(t: T) -> FutureResult<T, E> {
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
pub fn err<T, E>(e: E) -> FutureResult<T, E> {
    ready(Err(e))
}
