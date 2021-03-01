//! Definition of the `Result` (immediately finished) combinator
use std::marker::PhantomData;
use std::pin::Pin;
use std::task;
use std::task::Poll;

use crate::actor::Actor;
use crate::fut::ActorFuture;

/// Future for the [`ready`](ready()) function.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Ready<T, A> {
    inner: Option<T>,
    act: PhantomData<A>,
}

impl<T, A> Unpin for Ready<T, A> {}

/// Create a future that is immediately ready with a value.
pub fn ready<T, A>(r: T) -> Ready<T, A> {
    Ready {
        inner: Some(r),
        act: PhantomData,
    }
}

impl<T, A> ActorFuture for Ready<T, A>
where
    A: Actor,
{
    type Output = T;
    type Actor = A;

    #[inline]
    fn poll(
        mut self: Pin<&mut Self>,
        _: &mut Self::Actor,
        _: &mut <Self::Actor as Actor>::Context,
        _: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        Poll::Ready(self.inner.take().expect("cannot poll Result twice"))
    }
}
