use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

pin_project! {
    /// A stream combinator which chains a computation onto each item produced by a
    /// stream.
    ///
    /// This structure is produced by the `ActorStream::then` method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct StreamThen<S, Fn, F> {
        #[pin]
        stream: S,
        #[pin]
        future: Option<F>,
        f: Fn,
    }
}

pub fn new<S, A, Fn, F>(stream: S, f: Fn) -> StreamThen<S, Fn, F::Future>
where
    S: ActorStream<A>,
    A: Actor,
    F: IntoActorFuture<A>,
    Fn: FnMut(S::Item, &mut A, &mut A::Context) -> F,
{
    StreamThen {
        stream,
        f,
        future: None,
    }
}

impl<S, A, Fn, F> ActorStream<A> for StreamThen<S, Fn, F::Future>
where
    S: ActorStream<A>,
    A: Actor,
    F: IntoActorFuture<A>,
    Fn: FnMut(S::Item, &mut A, &mut A::Context) -> F,
{
    type Item = F::Output;

    fn poll_next(
        mut self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            let this = self.as_mut().project();
            match this.future.as_pin_mut() {
                None => {
                    let item = match this.stream.poll_next(act, ctx, task) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(None) => return Poll::Ready(None),
                        Poll::Ready(Some(e)) => e,
                    };
                    let fut = (this.f)(item, act, ctx).into_future();
                    self.as_mut().project().future.set(Some(fut));
                }
                Some(fut) => {
                    return match fut.poll(act, ctx, task) {
                        Poll::Ready(e) => {
                            self.project().future.set(None);
                            Poll::Ready(Some(e))
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
            }
        }
    }
}
