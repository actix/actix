use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

/// A stream combinator which chains a computation onto each item produced by a
/// stream.
///
/// This structure is produced by the `ActorStream::then` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamThen<S, F: 'static, U>
where
    U: IntoActorFuture,
    S: ActorStream,
{
    #[pin]
    stream: S,
    future: Option<U::Future>,
    f: F,
}

pub fn new<S, F: 'static, U>(stream: S, f: F) -> StreamThen<S, F, U>
where
    S: ActorStream,
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
    U: IntoActorFuture<Actor = S::Actor>,
{
    StreamThen {
        stream,
        f,
        future: None,
    }
}

impl<S, F: 'static, U> ActorStream for StreamThen<S, F, U>
where
    S: ActorStream + Unpin,
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
    U: IntoActorFuture<Actor = S::Actor>,
    U::Future: Unpin,
{
    type Item = U::Output;
    type Actor = S::Actor;

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<U::Output>> {
        let mut this = self.get_mut();
        if this.future.is_none() {
            let item = match Pin::new(&mut this.stream).poll_next(act, ctx, task) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Ready(Some(e)) => e,
            };
            this.future = Some((this.f)(item, act, ctx).into_future());
        }
        assert!(this.future.is_some());
        match Pin::new(this.future.as_mut().unwrap()).poll(act, ctx, task) {
            Poll::Ready(e) => {
                this.future = None;
                Poll::Ready(Some(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
