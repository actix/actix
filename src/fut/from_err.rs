use futures::{Async, Poll};
use std::marker::PhantomData;

use actor::Actor;
use fut::ActorFuture;

/// Future for the `from_err` combinator, changing the error type of a future.
///
/// This is created by the `Future::from_err` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct FromErr<A, E>
where
    A: ActorFuture,
{
    future: A,
    f: PhantomData<E>,
}

pub fn new<A, E>(future: A) -> FromErr<A, E>
where
    A: ActorFuture,
{
    FromErr {
        future,
        f: PhantomData,
    }
}

impl<A: ActorFuture, E: From<A::Error>> ActorFuture for FromErr<A, E> {
    type Item = A::Item;
    type Error = E;
    type Actor = A::Actor;

    fn poll(
        &mut self, act: &mut A::Actor, ctx: &mut <A::Actor as Actor>::Context,
    ) -> Poll<A::Item, E> {
        let e = match self.future.poll(act, ctx) {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            other => other,
        };
        e.map_err(From::from)
    }
}
