use std::{
    mem,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::ready;
use pin_project_lite::pin_project;

use super::ActorStream;
use crate::{actor::Actor, fut::future::ActorFuture};

pin_project! {
    /// Future for the [`collect`](super::ActorStreamExt::collect) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Collect<S, C> {
        #[pin]
        stream: S,
        collection: C,
    }
}

impl<S, C> Collect<S, C>
where
    C: Default,
{
    pub(super) fn new(stream: S) -> Self {
        Self {
            stream,
            collection: Default::default(),
        }
    }
}

impl<S, A, C> ActorFuture<A> for Collect<S, C>
where
    S: ActorStream<A>,
    A: Actor,
    C: Default + Extend<S::Item>,
{
    type Output = C;

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();
        loop {
            match ready!(this.stream.as_mut().poll_next(act, ctx, task)) {
                Some(e) => this.collection.extend(Some(e)),
                None => return Poll::Ready(mem::take(this.collection)),
            }
        }
    }
}
