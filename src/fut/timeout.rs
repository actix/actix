use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pin_project::pin_project;

use crate::actor::Actor;
use crate::clock::{delay_for, Delay};
use crate::fut::ActorFuture;

/// Future for the `timeout` combinator, interrupts computations if it takes
/// more than `timeout`.
///
/// This is created by the `ActorFuture::timeout()` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Timeout<F>
where
    F: ActorFuture,
{
    #[pin]
    fut: F,
    #[pin]
    timeout: Delay,
}

pub fn new<F>(future: F, timeout: Duration) -> Timeout<F>
where
    F: ActorFuture,
{
    Timeout {
        fut: future,
        timeout: delay_for(timeout),
    }
}

impl<F> ActorFuture for Timeout<F>
where
    F: ActorFuture,
{
    type Output = Result<F::Output, ()>;
    type Actor = F::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut F::Actor,
        ctx: &mut <F::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.project();

        if let Poll::Ready(_) = this.timeout.poll(task) {
            return Poll::Ready(Err(()));
        }

        this.fut.poll(act, ctx, task).map(Ok)
    }
}
