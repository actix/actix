use futures::Poll;

use super::chain::Chain;
use super::{ActorFuture, IntoActorFuture};
use actor::Actor;

/// Future for the `and_then` combinator, chaining a computation onto the end of
/// another future which completes successfully.
///
/// This is created by the `Future::and_then` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct AndThen<A, B, F>
where
    A: ActorFuture,
    B: IntoActorFuture<Actor = A::Actor>,
{
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F>(future: A, f: F) -> AndThen<A, B, F>
where
    A: ActorFuture,
    B: IntoActorFuture<Actor = A::Actor>,
{
    AndThen {
        state: Chain::new(future, f),
    }
}

impl<A, B, F> ActorFuture for AndThen<A, B, F>
where
    A: ActorFuture,
    B: IntoActorFuture<Actor = A::Actor, Error = A::Error>,
    F: FnOnce(A::Item, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> B,
{
    type Item = B::Item;
    type Error = B::Error;
    type Actor = A::Actor;

    fn poll(
        &mut self, act: &mut A::Actor, ctx: &mut <A::Actor as Actor>::Context,
    ) -> Poll<B::Item, B::Error> {
        self.state.poll(act, ctx, |result, f, act, ctx| {
            result.map(|e| Err(f(e, act, ctx).into_future()))
        })
    }
}
