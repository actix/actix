use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use pin_project_lite::pin_project;

use crate::{
    actor::Actor,
    fut::{future::ActorFuture, try_future::ActorTryFuture},
};

pin_project! {
    /// Future for the `and_then` combinator, chaining computations
    /// on the end of another actor future regardless of its outcome.
    ///
    /// This is created by the `ActorTryFuture::and_then` method.
    #[project = AndThenProj]
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
    A: ActorTryFuture<Act>,
    B: ActorTryFuture<Act>,
    Act: Actor,
{
    AndThen::First {
        fut1: future,
        data: Some(f),
    }
}

impl<A, B, F, Act> ActorFuture<Act> for AndThen<A, B, F>
where
    A: ActorTryFuture<Act>,
    B: ActorTryFuture<Act, Error = A::Error>,
    F: FnOnce(A::Ok, &mut Act, &mut Act::Context) -> B,
    Act: Actor,
{
    type Output = Result<B::Ok, A::Error>;

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut Act,
        ctx: &mut Act::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        match self.as_mut().project() {
            AndThenProj::First { fut1, data } => {
                let ok = ready!(fut1.try_poll(act, ctx, task))?;
                let data = data.take().unwrap();
                let fut2 = data(ok, act, ctx);
                self.set(AndThen::Second { fut2 });
                self.poll(act, ctx, task)
            }
            AndThenProj::Second { fut2 } => {
                let res = ready!(fut2.try_poll(act, ctx, task));
                self.set(AndThen::Empty);
                Poll::Ready(res)
            }
            AndThenProj::Empty => panic!("ActorFuture polled after finish"),
        }
    }
}
