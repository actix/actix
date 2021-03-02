use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

pin_project! {
    /// Stream for the [`then`](super::ActorStreamExt::then) method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Then<S, Fn, F> {
        #[pin]
        stream: S,
        #[pin]
        future: Option<F>,
        f: Fn,
    }
}

pub(super) fn new<S, A, Fn, F>(stream: S, f: Fn) -> Then<S, Fn, F::Future>
where
    S: ActorStream<A>,
    A: Actor,
    F: IntoActorFuture<A>,
    Fn: FnMut(S::Item, &mut A, &mut A::Context) -> F,
{
    Then {
        stream,
        f,
        future: None,
    }
}

impl<S, A, Fn, F> ActorStream<A> for Then<S, Fn, F::Future>
where
    S: ActorStream<A>,
    A: Actor,
    F: IntoActorFuture<A>,
    Fn: FnMut(S::Item, &mut A, &mut A::Context) -> F,
{
    type Item = F::Output;

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        Poll::Ready(loop {
            if let Some(fut) = this.future.as_mut().as_pin_mut() {
                let item = ready!(fut.poll(act, ctx, task));
                this.future.set(None);
                break Some(item);
            } else if let Some(item) = ready!(this.stream.as_mut().poll_next(act, ctx, task)) {
                this.future
                    .set(Some((this.f)(item, act, ctx).into_future()));
            } else {
                break None;
            }
        })
    }
}
