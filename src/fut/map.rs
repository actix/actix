use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project::pin_project;

use crate::actor::Actor;
use crate::fut::ActorFuture;

/// Future for the `map` combinator, changing the type of a future.
///
/// This is created by the `ActorFuture::map` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Map<A, F>
where
    A: ActorFuture,
{
    #[pin]
    future: A,
    f: Option<F>,
}

pub fn new<A, F>(future: A, f: F) -> Map<A, F>
where
    A: ActorFuture,
{
    Map { future, f: Some(f) }
}

impl<U, A, F> ActorFuture for Map<A, F>
where
    A: ActorFuture,
    F: FnOnce(A::Output, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> U,
{
    type Output = U;
    type Actor = A::Actor;
    fn poll(
        self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.project();
        let e = match this.future.poll(act, ctx, task) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(e) => e,
        };
        Poll::Ready(this.f.take().expect("cannot poll Map twice")(e, act, ctx))
    }
}
