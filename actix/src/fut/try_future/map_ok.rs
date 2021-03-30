use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::{
    fut::{future::ActorFuture, try_future::ActorTryFuture},
    Actor,
};

pin_project! {
    /// Future for the [`map`](super::ActorTryFutureExt::map_ok) method.
    #[project = MapProj]
    #[project_replace = MapProjReplace]
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub enum MapOk<Fut, F> {
        Incomplete {
            #[pin]
            future: Fut,
            f: F,
        },
        Complete,
    }
}

impl<Fut, F> MapOk<Fut, F> {
    pub(crate) fn new(future: Fut, f: F) -> Self {
        Self::Incomplete { future, f }
    }
}

impl<U, Fut, A, F> ActorFuture<A> for MapOk<Fut, F>
where
    Fut: ActorTryFuture<A>,
    A: Actor,
    F: FnOnce(Fut::Ok, &mut A, &mut A::Context) -> U,
{
    type Output = Result<U, Fut::Error>;

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        match self.as_mut().project() {
            MapProj::Incomplete { future, .. } => {
                let output = ready!(future.try_poll(act, ctx, task));
                match self.project_replace(MapOk::Complete) {
                    MapProjReplace::Incomplete { f, .. } => {
                        Poll::Ready(output.map(|ok| f(ok, act, ctx)))
                    }
                    MapProjReplace::Complete => unreachable!(),
                }
            }
            MapProj::Complete => {
                panic!("MapOk must not be polled after it returned `Poll::Ready`")
            }
        }
    }
}
