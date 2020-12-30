use std::pin::Pin;

use futures_util::task::{Context, Poll};

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream};

/// A combinator used to convert stream into a future, future resolves
/// when stream completes.
///
/// This structure is produced by the `ActorStream::finish` method.
#[must_use = "streams do nothing unless polled"]
#[pin_project::pin_project]
#[derive(Debug)]
pub struct StreamFinish<S: ActorStream>(#[pin] S);

pub fn new<S: ActorStream>(s: S) -> StreamFinish<S> {
    StreamFinish(s)
}

impl<S: ActorStream> ActorFuture for StreamFinish<S> {
    type Output = ();
    type Actor = S::Actor;

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<()> {
        loop {
            match self.as_mut().project().0.poll_next(act, ctx, task) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Ready(Some(_)) => (),
            };
        }
    }
}
