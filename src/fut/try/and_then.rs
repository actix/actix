use futures_util::ready;
use pin_project::pin_project;
use std::pin::Pin;
use std::task::{self, Context, Poll};

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorTryFuture, IntoActorFuture};

#[pin_project(project = ChainProj)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub enum AndThen<A, B, C> {
    First(#[pin] A, Option<C>),
    Second(#[pin] B),
    Empty,
}

impl<A, B, C> AndThen<A, B, C>
where
    A: ActorTryFuture,
    B: ActorTryFuture<Actor = A::Actor>,
{
    pub fn new(fut1: A, data: C) -> AndThen<A, B, C> {
        AndThen::First(fut1, Some(data))
    }
}

impl<A, B, F, R> ActorFuture for AndThen<A, B, F>
where
    A: ActorTryFuture,
    B: ActorFuture,
    R: IntoActorFuture<Actor = A::Actor>,
    F: FnOnce(A::Ok, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> R,
{
    type Output = Result<<R as IntoActorFuture::Output>, A::Error>;
    type Actor = A::Actor;

    fn poll(
        mut self: Pin<&mut Self>,
        srv: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Self::Output>
    where
        F: FnOnce(A::Output, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> B,
    {
        loop {
            let this = self.as_mut().project();
            let (output, mapper) = match this {
                ChainProj::First(fut1, data) => {
                    let output = ready!(fut1.try_poll(srv, ctx, task));
                    (output, data.take().unwrap())
                }

                ChainProj::Second(fut2) => {
                    return fut2.try_poll(srv, ctx, task);
                }

                ChainProj::Empty => unreachable!(),
            };

            self.set(AndThen::Empty);

            let fut2 = match mapper(t, srv, ctx) {
                Ok(t_fut) => t_fut,
                Err(err) => return Err(err),
            };

            self.set(AndThen::Second(fut2))
        }
    }
}
