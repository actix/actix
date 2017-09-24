use futures::{Async, Poll};

use fut::CtxFuture;
use context::Context;


/// Future for the `map` combinator, changing the type of a future.
///
/// This is created by the `Future::map` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Map<A, F> where A: CtxFuture {
    future: A,
    f: Option<F>,
}

pub fn new<A, F>(future: A, f: F) -> Map<A, F>
    where A: CtxFuture,
{
    Map {
        future: future,
        f: Some(f),
    }
}

impl<U, A, F> CtxFuture for Map<A, F>
    where A: CtxFuture,
          F: FnOnce(A::Item, &mut A::Service, &mut Context<A::Service>) -> U,
{
    type Item = U;
    type Error = A::Error;
    type Service = A::Service;

    fn poll(&mut self, srv: &mut Self::Service, ctx: &mut Context<Self::Service>) -> Poll<U, A::Error> {
        let e = match self.future.poll(srv, ctx) {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(e)) => Ok(e),
            Err(e) => Err(e),
        };
        match e {
            Ok(item) =>
                Ok(Async::Ready(
                    self.f.take().expect("cannot poll Map twice")(item, srv, ctx))),
            Err(err) => Err(err)
        }
    }
}
