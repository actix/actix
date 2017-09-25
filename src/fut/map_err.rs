use futures::{Async, Poll};

use fut::ActorFuture;
use context::Context;


/// Future for the `map_err` combinator, changing the error type of a future.
///
/// This is created by the `Future::map_err` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct MapErr<A, F> where A: ActorFuture {
    future: A,
    f: Option<F>,
}

pub fn new<A, F>(future: A, f: F) -> MapErr<A, F> where A: ActorFuture
{
    MapErr {
        future: future,
        f: Some(f),
    }
}

impl<U, A, F> ActorFuture for MapErr<A, F>
    where A: ActorFuture,
          F: FnOnce(A::Error, &mut A::Actor, &mut Context<A::Actor>) -> U,
{
    type Item = A::Item;
    type Error = U;
    type Actor = A::Actor;

    fn poll(&mut self, act: &mut A::Actor, ctx: &mut Context<A::Actor>) -> Poll<A::Item, U> {
        let e = match self.future.poll(act, ctx) {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            other => other,
        };
        match e {
            Err(e) =>
                Err(self.f.take().expect("cannot poll MapErr twice")(e, act, ctx)),
            Ok(err) => Ok(err)
        }
    }
}
