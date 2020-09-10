use std::{pin::Pin, task, task::Poll};

use futures_util::ready;
use pin_project::pin_project;

use crate::{Actor, ActorFuture, ActorTryFuture};

#[pin_project]
#[derive(Debug)]
pub struct MapErr<A, F> {
    #[pin]
    future: A,
    f: Option<F>,
}

impl<A, F> MapErr<A, F>
where
    A: ActorTryFuture,
{
    pub fn new(future: A, f: F) -> Self {
        Self { future, f: Some(f) }
    }
}

impl<F, R, A> ActorFuture for MapErr<A, F>
where
    A: ActorTryFuture,
    F: FnOnce(A::Error, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> R,
{
    type Output = Result<A::Ok, R>;
    type Actor = A::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Result<A::Ok, R>> {
        let this = self.project();
        let res = ready!(this.future.try_poll(act, ctx, task));

        let f = this.f.take().expect("cannot poll MapErr twice");
        let mapped = res.map_err(|err| f(err, act, ctx));
        Poll::Ready(mapped)
    }
}
