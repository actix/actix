use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

pin_project! {
    /// Stream for the [`fold`](super::ActorStreamExt::fold) method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Fold<S, F, Fut, T> {
        #[pin]
        stream: S,
        f: F,
        accum: Option<T>,
        #[pin]
        future: Option<Fut>,
    }
}

pub(super) fn new<S, A, F, Fut>(
    stream: S,
    f: F,
    t: Fut::Output,
) -> Fold<S, F, Fut::Future, Fut::Output>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(Fut::Output, S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: IntoActorFuture<A>,
{
    Fold {
        stream,
        f,
        accum: Some(t),
        future: None,
    }
}

impl<S, A, F, Fut, T> ActorFuture<A> for Fold<S, F, Fut::Future, T>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(T, S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: IntoActorFuture<A, Output = T>,
    Fut::Future: ActorFuture<A>,
{
    type Output = T;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<T> {
        let mut this = self.project();
        Poll::Ready(loop {
            if let Some(fut) = this.future.as_mut().as_pin_mut() {
                // we're currently processing a future to produce a new accum value
                *this.accum = Some(ready!(fut.poll(act, ctx, task)));
                this.future.set(None);
            } else if this.accum.is_some() {
                // we're waiting on a new item from the stream
                let res = ready!(this.stream.as_mut().poll_next(act, ctx, task));
                let a = this.accum.take().unwrap();
                if let Some(item) = res {
                    this.future
                        .set(Some((this.f)(a, item, act, ctx).into_future()));
                } else {
                    break a;
                }
            } else {
                panic!("Fold polled after completion")
            }
        })
    }
}
