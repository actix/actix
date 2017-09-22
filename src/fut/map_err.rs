use futures::{Async, Poll};

use fut::CtxFuture;
use service::Service;

/// Future for the `map_err` combinator, changing the error type of a future.
///
/// This is created by the `Future::map_err` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct MapErr<A, F> where A: CtxFuture {
    future: A,
    f: Option<F>,
}

pub fn new<A, F>(future: A, f: F) -> MapErr<A, F>
    where A: CtxFuture
{
    MapErr {
        future: future,
        f: Some(f),
    }
}

impl<U, A, F> CtxFuture for MapErr<A, F>
    where A: CtxFuture,
          F: FnOnce(A::Error, &mut A::Service, &mut <A::Service as Service>::Context) -> U,
{
    type Item = A::Item;
    type Error = U;
    type Service = A::Service;

    fn poll(&mut self, srv: &mut A::Service, ctx: &mut <A::Service as Service>::Context) -> Poll<A::Item, U> {
        let e = match self.future.poll(srv, ctx) {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            other => other,
        };
        match e {
            Err(e) =>
                Err(self.f.take().expect("cannot poll MapErr twice")(e, srv, ctx)),
            Ok(err) => Ok(err)
        }
    }
}
