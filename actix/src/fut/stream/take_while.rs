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
    /// Stream for the [`take_while`](super::ActorStreamExt::take_while) method.
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct TakeWhile<S, I, F, Fut> {
        #[pin]
        stream: S,
        f: F,
        #[pin]
        pending_fut: Option<Fut>,
        pending_item: Option<I>,
        done_taking: bool,
    }
}

pub(super) fn new<S, A, F, Fut>(stream: S, f: F) -> TakeWhile<S, S::Item, F, Fut>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(&S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: ActorFuture<A, Output = bool>,
{
    TakeWhile {
        stream,
        f,
        pending_fut: None,
        pending_item: None,
        done_taking: false,
    }
}

impl<S, A, F, Fut> ActorStream<A> for TakeWhile<S, S::Item, F, Fut>
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
        if self.done_taking {
            return Poll::Ready(None);
        }

        let mut this = self.project();

        Poll::Ready(loop {
            if let Some(fut) = this.pending_fut.as_mut().as_pin_mut() {
                let take = ready!(fut.poll(act, ctx, task));
                let item = this.pending_item.take();
                this.pending_fut.set(None);
                if take {
                    break item;
                } else {
                    *this.done_taking = true;
                    break None;
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
