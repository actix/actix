use futures::task::{Context, Poll};

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream};
use pin_project::{pin_project, project};
use std::pin::Pin;

/// A combinator used to convert stream into a future, future resolves
/// when stream completes.
///
/// This structure is produced by the `ActorStream::finish` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamFinish<S: ActorStream + Unpin>(#[pin] S);

pub fn new<S>(s: S) -> StreamFinish<S>
where
    S: ActorStream + Unpin,
{
    StreamFinish(s)
}

impl<S: ActorStream> ActorFuture for StreamFinish<S>
where
    S: ActorStream + Unpin,
    Self: Unpin,
{
    type Output = ();
    type Actor = S::Actor;

    #[project]
    fn poll(
        self: Pin<&mut Self>,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<()> {
        let this = self.get_mut();
        loop {
            match Pin::new(&mut this.0).poll_next(act, ctx, task) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Ready(Some(_)) => (),
            };
        }
    }
}
