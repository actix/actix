use futures::task::Poll;
use pin_project::pin_project;

use crate::actor::Actor;
use crate::fut::chain::Chain;
use crate::fut::{ActorFuture, IntoActorFuture};
use std::pin::Pin;
use std::task;

/// Future for the `then` combinator, chaining computations on the end of
/// another future regardless of its outcome.
///
/// This is created by the `Future::then` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Then<A, B, F>
where
    A: ActorFuture,
    B: IntoActorFuture<Actor = A::Actor>,
{
    #[pin]
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F>(future: A, f: F) -> Then<A, B, F>
where
    A: ActorFuture,
    B: IntoActorFuture<Actor = A::Actor>,
{
    Then {
        state: Chain::new(future, f),
    }
}

impl<A, B, F> ActorFuture for Then<A, B, F>
where
    A: ActorFuture,
    B: IntoActorFuture<Actor = A::Actor>,
    F: FnOnce(A::Item, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> B,
{
    type Item = B::Item;
    type Actor = A::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<B::Item> {
        self.project()
            .state
            .poll(act, ctx, task, |item, f, act, ctx| {
                // This is not an error, just the second variant of the enum
                Err(f(item, act, ctx).into_future())
            })
    }
}
