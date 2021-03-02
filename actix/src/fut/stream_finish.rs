use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream};

pin_project! {
    /// A combinator used to convert stream into a future, future resolves
    /// when stream completes.
    ///
    /// This structure is produced by the `ActorStream::finish` method.
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct StreamFinish<S> {
        #[pin]
        stream: S
    }
}

pub fn new<S>(stream: S) -> StreamFinish<S> {
    StreamFinish { stream }
}

impl<S, A> ActorFuture<A> for StreamFinish<S>
where
    S: ActorStream<A>,
    A: Actor,
{
    type Output = ();

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<()> {
        loop {
            match self.as_mut().project().stream.poll_next(act, ctx, task) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Ready(Some(_)) => (),
            };
        }
    }
}
