use std::pin::Pin;
use std::task::{self, Poll};

use pin_project::pin_project;

use crate::actor::Actor;
use crate::fut::chain::Chain;
use crate::fut::{ActorFuture, IntoActorFuture};

/// Future for the `then` combinator, chaining computations on the end of
/// another future regardless of its outcome.
///
/// This is created by the `Future::then` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Then<A, B, F: 'static>
where
    A: ActorFuture,
    B: IntoActorFuture<Actor = A::Actor>,
{
    #[pin]
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F: 'static>(future: A, f: F) -> Then<A, B, F>
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
    F: FnOnce(A::Output, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> B,
{
    type Output = B::Output;
    type Actor = A::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<B::Output> {
        self.project()
            .state
            .poll(act, ctx, task, |item, f, act, ctx| {
                f(item, act, ctx).into_future()
            })
    }
}
