use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project::pin_project;

use crate::actor::Actor;
use crate::fut::ActorStream;

/// A stream combinator which will change the type of a stream from one
/// type to another.
///
/// This is produced by the `ActorStream::map` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamMap<S, F> {
    #[pin]
    stream: S,
    f: F,
}

pub fn new<S, F, U>(stream: S, f: F) -> StreamMap<S, F>
where
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
    S: ActorStream,
{
    StreamMap { stream, f }
}

impl<S, F, U> ActorStream for StreamMap<S, F>
where
    S: ActorStream,
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
{
    type Item = U;
    type Actor = S::Actor;

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
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
