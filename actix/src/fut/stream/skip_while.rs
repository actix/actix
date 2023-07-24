use std::{
    pin::Pin,
    task::{self, Poll},
};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::{
    actor::Actor,
    fut::{ActorFuture, ActorStream},
};

pin_project! {
    /// Stream for the [`skip_while`](super::ActorStreamExt::skip_while) method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct SkipWhile<S, I, F, Fut> {
        #[pin]
        stream: S,
        f: F,
        #[pin]
        pending_fut: Option<Fut>,
        pending_item: Option<I>,
        done_skipping: bool,
    }
}

pub(super) fn new<S, A, F, Fut>(stream: S, f: F) -> SkipWhile<S, S::Item, F, Fut>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(&S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: ActorFuture<A, Output = bool>,
{
    SkipWhile {
        stream,
        f,
        pending_fut: None,
        pending_item: None,
        done_skipping: false,
    }
}

impl<S, A, F, Fut> ActorStream<A> for SkipWhile<S, S::Item, F, Fut>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(&S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: ActorFuture<A, Output = bool>,
{
    type Item = S::Item;

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if *this.done_skipping {
            return this.stream.poll_next(act, ctx, task);
        }

        Poll::Ready(loop {
            if let Some(fut) = this.pending_fut.as_mut().as_pin_mut() {
                let skipped = ready!(fut.poll(act, ctx, task));
                let item = this.pending_item.take();
                this.pending_fut.set(None);
                if !skipped {
                    *this.done_skipping = true;
                    break item;
                }
            } else if let Some(item) = ready!(this.stream.as_mut().poll_next(act, ctx, task)) {
                this.pending_fut.set(Some((this.f)(&item, act, ctx)));
                *this.pending_item = Some(item);
            } else {
                break None;
            }
        })
    }
}
