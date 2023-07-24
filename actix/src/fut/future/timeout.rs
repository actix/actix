use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use pin_project_lite::pin_project;

use crate::{
    actor::Actor,
    clock::{sleep, Sleep},
    fut::ActorFuture,
};

pin_project! {
    /// Future for the [`timeout`](super::ActorFutureExt::timeout) combinator, interrupts computations if it takes
    /// more than [`timeout`](super::ActorFutureExt::timeout).
    ///
    /// This is created by the [`timeout`](super::ActorFutureExt::timeout) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless polled"]
    pub struct Timeout<F>{
        #[pin]
        fut: F,
        #[pin]
        timeout: Sleep,
    }
}

impl<F> Timeout<F> {
    pub(super) fn new(future: F, timeout: Duration) -> Self {
        Self {
            fut: future,
            timeout: sleep(timeout),
        }
    }
}

impl<F, A> ActorFuture<A> for Timeout<F>
where
    F: ActorFuture<A>,
    A: Actor,
{
    type Output = Result<F::Output, ()>;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.project();
        match this.fut.poll(act, ctx, task) {
            Poll::Ready(res) => Poll::Ready(Ok(res)),
            Poll::Pending => this.timeout.poll(task).map(Err),
        }
    }
}
