use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::ActorFuture;

pin_project! {
    /// Future for the [`map`](super::ActorFutureExt::map) method.
    #[project = MapProj]
    #[project_replace = MapProjReplace]
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub enum Map<Fut, Fn> {
        Incomplete {
            #[pin]
            future: Fut,
            f: Fn,
        },
        Complete,
    }
}

impl<Fut, Fn> Map<Fut, Fn> {
    pub(super) fn new(future: Fut, f: Fn) -> Self {
        Self::Incomplete { future, f }
    }
}

impl<U, Fut, A, Fn> ActorFuture<A> for Map<Fut, Fn>
where
    Fut: ActorFuture<A>,
    A: Actor,
    Fn: FnOnce(Fut::Output, &mut A, &mut A::Context) -> U,
{
    type Output = U;

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        match self.as_mut().project() {
            MapProj::Incomplete { future, .. } => {
                let output = ready!(future.poll(act, ctx, task));
                match self.project_replace(Map::Complete) {
                    MapProjReplace::Incomplete { f, .. } => Poll::Ready(f(output, act, ctx)),
                    MapProjReplace::Complete => unreachable!(),
                }
            }
            MapProj::Complete => {
                panic!("Map must not be polled after it returned `Poll::Ready`")
            }
        }
    }
}
