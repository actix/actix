use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::clock::{sleep, Instant, Sleep};
use crate::fut::ActorStream;

pin_project! {
    /// Stream for the [`timeout`](super::ActorStreamExt::timeout) method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Timeout<S> {
        #[pin]
        stream: S,
        dur: Duration,
        reset_timeout: bool,
        #[pin]
        timeout: Sleep,
    }
}

impl<S> Timeout<S> {
    pub(super) fn new(stream: S, timeout: Duration) -> Self {
        Self {
            stream,
            dur: timeout,
            reset_timeout: false,
            timeout: sleep(timeout),
        }
    }
}

impl<S, A> ActorStream<A> for Timeout<S>
where
    S: ActorStream<A>,
    A: Actor,
{
    type Item = Result<S::Item, ()>;

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Result<S::Item, ()>>> {
        let mut this = self.project();

        match this.stream.poll_next(act, ctx, task) {
            Poll::Ready(Some(res)) => {
                *this.reset_timeout = true;
                Poll::Ready(Some(Ok(res)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => {
                // only reset timeout when poll_next returns Ready and followed by Pending after.
                if *this.reset_timeout {
                    *this.reset_timeout = false;
                    this.timeout.as_mut().reset(Instant::now() + *this.dur);
                }

                // check timeout
                this.timeout.poll(task).map(|_| Some(Err(())))
            }
        }
    }
}
