use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream};

pin_project! {
    /// Future for the [`finish`](super::ActorStreamExt::finish) method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Finish<S> {
        #[pin]
        pub(crate) stream: S
    }
}

impl<S> Finish<S> {
    pub fn new(stream: S) -> Finish<S> {
        Finish { stream }
    }
}

impl<S, A> ActorFuture<A> for Finish<S>
where
    S: ActorStream<A>,
    A: Actor,
{
    type Output = ();

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<()> {
        let mut this = self.as_mut().project();
        while ready!(this.stream.as_mut().poll_next(act, ctx, task)).is_some() {}
        Poll::Ready(())
    }
}
