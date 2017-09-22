//! Definition of the `Result` (immediately finished) combinator

use std::marker::PhantomData;
use futures::{Poll, Async};
use fut::CtxFuture;
use service::Service;


/// A future representing a value that is immediately ready.
///
/// Created by the `result` function.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
// TODO: rename this to `Result` on the next major version
pub struct FutureResult<T, E, S> {
    inner: Option<Result<T, E>>,
    srv: PhantomData<S>,
}

/// Creates a new "leaf future" which will resolve with the given result.
///
/// The returned future represents a computation which is finshed immediately.
/// This can be useful with the `finished` and `failed` base future types to
/// convert an immediate value to a future to interoperate elsewhere.
///
/// # Examples
///
/// ```
/// use ctx::fut::*;
///
/// let future_of_1 = result::<u32, u32>(Ok(1));
/// let future_of_err_2 = result::<u32, u32>(Err(2));
/// ```
pub fn result<T, E, S>(r: Result<T, E>) -> FutureResult<T, E, S> {
    FutureResult { inner: Some(r), srv: PhantomData }
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
/// use ctx::fut::*;
///
/// let future_of_1 = ok::<u32, u32>(1);
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
/// use futures::future::*;
///
/// let future_of_err_1 = err::<u32, u32>(1);
/// ```
pub fn err<T, E, S>(e: E) -> FutureResult<T, E, S> {
    result(Err(e))
}

impl<T, E, S> CtxFuture for FutureResult<T, E, S> where S: Service {
    type Item = T;
    type Error = E;
    type Service = S;

    fn poll(&mut self, _: &mut Self::Service,
            _: &mut <Self::Service as Service>::Context) -> Poll<T, E>
    {
        self.inner.take().expect("cannot poll Result twice").map(Async::Ready)
    }
}
