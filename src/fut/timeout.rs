use futures::{
    task::{Context, Poll},
    Future,
};
use tokio::time::Delay;

use std::pin::Pin;
use std::time::Duration;

use pin_project::pin_project;

use crate::actor::Actor;
use crate::clock;
use crate::fut::ActorFuture;

/// Future for the `timeout` combinator, interrupts computations if it takes
/// more than `timeout`.
///
/// This is created by the `ActorFuture::timeout()` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Timeout<F, E>
where
    F: ActorFuture,
{
    #[pin]
    fut: F,
    err: Option<E>,
    #[pin]
    timeout: Delay,
}

pub fn new<F, E>(future: F, timeout: Duration, err: E) -> Timeout<F, E>
where
    F: ActorFuture,
{
    Timeout {
        fut: future,
        err: Some(err),
        timeout: tokio::time::delay_for(timeout),
    }
}

impl<F, E> ActorFuture for Timeout<F, E>
where
    F: ActorFuture,
{
    type Output = Result<F::Output, E>;
    type Actor = F::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut F::Actor,
        ctx: &mut <F::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.project();

        match this.timeout.poll(task) {
            Poll::Ready(_) => return Poll::Ready(Err(this.err.take().unwrap())),
            _ => {}
        }
        this.fut.poll(act, ctx, task).map(Ok)
    }
}
