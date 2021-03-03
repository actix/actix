use std::pin::Pin;
use std::task::{self, Poll};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::ActorFuture;

pin_project! {
    /// Future for the `then` combinator, chaining computations on the end of
    /// another future regardless of its outcome.
    ///
    /// This is created by the `Future::then` method.
    #[project = ThenProj]
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub enum Then<A, B, Fn> {
        First {
            #[pin]
            fut1: A,
            data: Option<Fn>,
        },
        Second {
            #[pin]
            fut2: B
        },
        Empty,
    }
}

pub(super) fn new<A, B, Fn, Act>(future: A, f: Fn) -> Then<A, B, Fn>
where
    A: ActorFuture<Act>,
    B: ActorFuture<Act>,
    Act: Actor,
{
    Then::First {
        fut1: future,
        data: Some(f),
    }
}

impl<A, B, Fn, Act> ActorFuture<Act> for Then<A, B, Fn>
where
    A: ActorFuture<Act>,
    B: ActorFuture<Act>,
    Fn: FnOnce(A::Output, &mut Act, &mut Act::Context) -> B,
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
                let output = ready!(fut1.poll(act, ctx, task));
                let data = data.take().unwrap();
                let fut2 = data(output, act, ctx);
                self.set(Then::Second { fut2 });
                self.poll(act, ctx, task)
            }
            ThenProj::Second { fut2 } => {
                let res = ready!(fut2.poll(act, ctx, task));
                self.set(Then::Empty);
                Poll::Ready(res)
            }
            ThenProj::Empty => panic!("ActorFuture polled after finish"),
        }
    }
}
