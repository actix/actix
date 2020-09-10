use std::{pin::Pin, task, task::Poll};

use futures_util::ready;
use pin_project::pin_project;

use crate::{Actor, ActorFuture, ActorTryFuture};

#[pin_project]
#[derive(Debug)]
pub struct MapOk<A, F> {
    #[pin]
    future: A,
    f: Option<F>,
}

impl<A, F> MapOk<A, F>
where
    A: ActorTryFuture,
{
    pub fn new(future: A, f: F) -> Self {
        Self { future, f: Some(f) }
    }
}

impl<F, R, A> ActorFuture for MapOk<A, F>
where
    A: ActorTryFuture,
    F: FnOnce(A::Ok, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> R,
{
    type Output = Result<R, A::Error>;
    type Actor = A::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Result<R, A::Error>> {
        let this = self.project();
        let res = ready!(this.future.try_poll(act, ctx, task));

        let f = this.f.take().expect("cannot poll MapOk twice");
        let mapped = res.map(|ok| f(ok, act, ctx));
        Poll::Ready(mapped)
    }
}
