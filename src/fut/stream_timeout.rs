use futures::{
    task::{Context, Poll},
    Future,
};
use std::pin::Pin;
use std::time::Duration;

use pin_project::{pin_project, project};
use tokio::time::{delay_for, Delay};

use crate::actor::Actor;
use crate::clock;
use crate::fut::ActorStream;

/// Future for the `timeout` combinator, interrupts computations if it takes
/// more than `timeout`.
///
/// This is created by the `ActorFuture::timeout()` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamTimeout<S, E>
where
    S: ActorStream,
    E: Clone,
{
    #[pin]
    stream: S,
    err: E,
    dur: Duration,
    #[pin]
    timeout: Delay,
}

pub fn new<S, E>(stream: S, timeout: Duration, err: E) -> StreamTimeout<S, E>
where
    S: ActorStream,
    E: Clone,
{
    StreamTimeout {
        stream,
        err,
        dur: timeout,
        timeout: tokio::time::delay_for(timeout),
    }
}

impl<S, E> ActorStream for StreamTimeout<S, E>
where
    S: ActorStream,
    E: Clone,
{
    type Item = Result<S::Item, E>;
    type Actor = S::Actor;

    #[project]
    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.project();
        match this.stream.poll_next(act, ctx, task) {
            Poll::Ready(Some(res)) => {
                return Poll::Ready(Some(Ok(res)));
            }
            Poll::Ready(None) => {
                return Poll::Ready(None);
            }
            Poll::Pending => (),
        }

        // check timeout
        match this.timeout.poll(task) {
            Poll::Ready(_) => Poll::Ready(Some(Err(this.err.clone()))),
            Poll::Pending => return Poll::Pending,
        }
    }
}
