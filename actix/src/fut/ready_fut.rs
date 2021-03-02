//! Definition of the `Result` (immediately finished) combinator
use std::future::Future;
use std::pin::Pin;
use std::task;
use std::task::Poll;

use crate::actor::Actor;
use crate::fut::ActorFuture;

pub use futures_util::future::{ready, Ready};

impl<T, A> ActorFuture<A> for Ready<T>
where
    A: Actor,
{
    type Output = T;

    #[inline]
    fn poll(
        self: Pin<&mut Self>,
        _: &mut A,
        _: &mut A::Context,
        cx: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        Future::poll(self, cx)
    }
}
