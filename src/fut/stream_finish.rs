use futures::{
    task::{self, Poll},
    Future,
};

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream};
use pin_project::{pin_project, project, project_ref};
use std::pin::Pin;

/// A combinator used to convert stream into a future, future resolves
/// when stream completes.
///
/// This structure is produced by the `ActorStream::finish` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamFinish<S: ActorStream>(#[pin] S);

pub fn new<S>(s: S) -> StreamFinish<S>
where
    S: ActorStream,
{
    StreamFinish(s)
}

impl<S: ActorStream> ActorFuture for StreamFinish<S>
where
    S: ActorStream,
{
    type Item = ();
    type Actor = S::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<()> {
        // let this = self.as_mut();
        // loop {
        //     match this.0.poll(act, ctx, task) {
        //         Poll::Pending => return Poll::Pending,
        //         Poll::Ready(None) => return Poll::Ready(()),
        //         Poll::Ready(Some(_)) => (),
        //     };
        // }
        Poll::Ready(())
    }
}
