use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::clock::{self, Sleep};
use crate::fut::ActorStream;

pin_project! {
    /// Future for the `timeout` combinator, interrupts computations if it takes
    /// more than `timeout`.
    ///
    /// This is created by the `ActorFuture::timeout()` method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct StreamTimeout<S> {
        #[pin]
        stream: S,
        dur: Duration,
        #[pin]
        timeout: Option<Sleep>,
    }
}

pub fn new<S, A>(stream: S, timeout: Duration) -> StreamTimeout<S>
where
    S: ActorStream<A>,
    A: Actor,
{
    StreamTimeout {
        stream,
        dur: timeout,
        timeout: None,
    }
}

impl<S, A> ActorStream<A> for StreamTimeout<S>
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
                this.timeout.set(None);
                return Poll::Ready(Some(Ok(res)));
            }
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => (),
        }

        if this.timeout.is_none() {
            this.timeout.set(Some(clock::sleep(*this.dur)));
        }

        // check timeout
        if this
            .timeout
            .as_mut()
            .as_pin_mut()
            .unwrap()
            .poll(task)
            .is_pending()
        {
            return Poll::Pending;
        }
        this.timeout.set(None);

        Poll::Ready(Some(Err(())))
    }
}
