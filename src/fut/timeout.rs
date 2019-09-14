use std::time::Duration;
use std::future::Future;
use std::task::Poll;
use std::pin::Pin;

use tokio_timer::Delay;

use crate::actor::Actor;
use crate::clock;
use crate::fut::ActorFuture;
use std::task;

/// Future for the `timeout` combinator, interrupts computations if it takes
/// more than `timeout`.
///
/// This is created by the `ActorFuture::timeout()` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Timeout<F,E>
where
    F: ActorFuture,
{
    fut: F,
    err: Option<E>,
    timeout: Delay,
}

pub fn new<F, E>(future: F, timeout: Duration, err: E) -> Timeout<F, E>
where
    F: ActorFuture,
{
    Timeout {
        fut: future,
        err: Some(err),
        timeout: tokio_timer::delay(clock::now() + timeout),
    }
}

impl<F, E> ActorFuture for Timeout<F, E>
where
    F: ActorFuture,
{
    type Item = Result<F::Item, E>;
    type Actor = F::Actor;

    fn poll(
        &mut self,
        act: &mut F::Actor,
        ctx: &mut <F::Actor as Actor>::Context,
        task : &mut task::Context<'_>
    ) -> Poll<Self::Item> {
        // check timeout
        match unsafe { Pin::new_unchecked(&mut self.timeout) }.poll(task) {
            Poll::Ready(()) => return Poll::Ready(Err(self.err.take().unwrap())),
            Poll::Pending => (),
        }

        self.fut.poll(act, ctx, task).map(Ok)
    }
}
