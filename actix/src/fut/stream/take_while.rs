use std::pin::Pin;
use std::task::{self, Poll};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

pin_project! {
    /// Stream for the [`take_while`](super::ActorStreamExt::take_while) method.
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct TakeWhile<S, I, Fut, Fn> {
        #[pin]
        stream: S,
        f: Fn,
        #[pin]
        pending_fut: Option<Fut>,
        pending_item: Option<I>,
        done_taking: bool,
    }
}

pub(super) fn new<S, A, Fut, Fn>(stream: S, f: Fn) -> TakeWhile<S, S::Item, Fut::Future, Fn>
where
    S: ActorStream<A>,
    A: Actor,
    Fn: FnMut(&S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: IntoActorFuture<A, Output = bool>,
{
    TakeWhile {
        stream,
        f,
        pending_fut: None,
        pending_item: None,
        done_taking: false,
    }
}

impl<S, A, Fut, Fn> ActorStream<A> for TakeWhile<S, S::Item, Fut::Future, Fn>
where
    S: ActorStream<A>,
    A: Actor,
    Fn: FnMut(&S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: IntoActorFuture<A, Output = bool>,
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
                this.pending_fut
                    .set(Some((this.f)(&item, act, ctx).into_future()));
                *this.pending_item = Some(item);
            } else {
                break None;
            }
        })
    }
}
