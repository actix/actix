use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::ActorStream;

pin_project! {
    /// A stream combinator which will change the type of a stream from one
    /// type to another.
    ///
    /// This is produced by the `ActorStream::map` method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct StreamMap<S, F> {
        #[pin]
        stream: S,
        f: F,
    }
}

pub fn new<S, A, F, U>(stream: S, f: F) -> StreamMap<S, F>
where
    F: FnMut(S::Item, &mut A, &mut A::Context) -> U,
    S: ActorStream<A>,
    A: Actor,
{
    StreamMap { stream, f }
}

impl<S, A, F, U> ActorStream<A> for StreamMap<S, F>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(S::Item, &mut A, &mut A::Context) -> U,
{
    type Item = U;

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<U>> {
        let this = self.project();
        match this.stream.poll_next(act, ctx, task) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(option) => {
                if let Some(item) = option {
                    Poll::Ready(Some((this.f)(item, act, ctx)))
                } else {
                    Poll::Ready(None)
                }
            }
        }
    }
}
