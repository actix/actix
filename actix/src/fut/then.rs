use std::pin::Pin;
use std::task::{self, Poll};

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::chain::Chain;
use crate::fut::{ActorFuture, IntoActorFuture};

pin_project! {
    /// Future for the `then` combinator, chaining computations on the end of
    /// another future regardless of its outcome.
    ///
    /// This is created by the `Future::then` method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub struct Then<A, B, Fn, Act>
    where
        A: ActorFuture<Act>,
        B: IntoActorFuture<Act>,
        Act: Actor,
    {
        #[pin]
        state: Chain<A, B::Future, Fn, Act>,
    }
}

pub fn new<A, B, Fn, Act>(future: A, f: Fn) -> Then<A, B, Fn, Act>
where
    A: ActorFuture<Act>,
    B: IntoActorFuture<Act>,
    Act: Actor,
{
    Then {
        state: Chain::new(future, f),
    }
}

impl<A, B, Fn, Act> ActorFuture<Act> for Then<A, B, Fn, Act>
where
    A: ActorFuture<Act>,
    B: IntoActorFuture<Act>,
    Fn: FnOnce(A::Output, &mut Act, &mut Act::Context) -> B,
    Act: Actor,
{
    type Output = B::Output;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut Act,
        ctx: &mut Act::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<B::Output> {
        self.project()
            .state
            .poll(act, ctx, task, |item, f, act, ctx| {
                f(item, act, ctx).into_future()
            })
    }
}
