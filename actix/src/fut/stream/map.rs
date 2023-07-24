use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::{actor::Actor, fut::ActorStream};

pin_project! {
    /// Stream for the [`map`](super::ActorStreamExt::map) method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Map<S, F> {
        #[pin]
        stream: S,
        f: F,
    }
}

pub(super) fn new<S, A, F, U>(stream: S, f: F) -> Map<S, F>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(S::Item, &mut A, &mut A::Context) -> U,
{
    Map { stream, f }
}

impl<S, A, F, U> ActorStream<A> for Map<S, F>
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
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let res = ready!(this.stream.as_mut().poll_next(act, ctx, task));
        Poll::Ready(res.map(|x| (this.f)(x, act, ctx)))
    }
}
