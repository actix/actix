use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::ActorFuture;

pin_project! {
    /// Future for the `map` combinator, changing the type of a future.
    ///
    /// This is created by the `ActorFuture::map` method
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub struct Map<F, Fn> {
        #[pin]
        future: F,
        f: Option<Fn>,
    }
}

pub fn new<F, Fn>(future: F, f: Fn) -> Map<F, Fn> {
    Map { future, f: Some(f) }
}

impl<U, F, A, Fn> ActorFuture<A> for Map<F, Fn>
where
    F: ActorFuture<A>,
    A: Actor,
    Fn: FnOnce(F::Output, &mut A, &mut A::Context) -> U,
{
    type Output = U;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
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
