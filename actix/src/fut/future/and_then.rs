use std::pin::Pin;
use std::task::{self, Poll};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::ActorFuture;

pin_project! {
    /// Future for the `and_then` combinator, chaining computations on the end of
    /// another future regardless of its outcome.
    ///
    /// This is created by the `Future::and_then` method.
    #[project = ThenProj]
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub enum AndThen<A, B, F> {
        First {
            #[pin]
            fut1: A,
            data: Option<F>,
        },
        Second {
            #[pin]
            fut2: B
        },
        Empty,
    }
}

pub(super) fn new<A, B, F, Act>(future: A, f: F) -> AndThen<A, B, F>
where
    A: ActorFuture<Act>,
    B: ActorFuture<Act>,
    Act: Actor,
{
    AndThen::First {
        fut1: future,
        data: Some(f),
    }
}

impl<A, B, F, Act, TA, TB, E> ActorFuture<Act> for AndThen<A, B, F>
where
    A: ActorFuture<Act, Output = Result<TA, E>>,
    B: ActorFuture<Act, Output = Result<TB, E>>,
    F: FnOnce(TA, &mut Act, &mut Act::Context) -> B,
    Act: Actor,
{
    type Output = B::Output;

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut Act,
        ctx: &mut Act::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<B::Output> {
        match self.as_mut().project() {
            ThenProj::First { fut1, data } => {
                let res = ready!(fut1.poll(act, ctx, task))?;
                let data = data.take().unwrap();
                let fut2 = data(res, act, ctx);
                self.set(AndThen::Second { fut2 });
                self.poll(act, ctx, task)
            }
            ThenProj::Second { fut2 } => {
                let res = ready!(fut2.poll(act, ctx, task));
                self.set(AndThen::Empty);
                Poll::Ready(res)
            }
            ThenProj::Empty => panic!("ActorFuture polled after finish"),
        }
    }
}
