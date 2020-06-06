use pin_project::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::actor::Actor;
use crate::fut::ActorFuture;

#[pin_project(project = ChainProj)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub enum Chain<A, B, C> {
    First(#[pin] A, Option<C>),
    Second(#[pin] B),
    Empty,
}

impl<A, B, C> Chain<A, B, C>
where
    A: ActorFuture,
    B: ActorFuture<Actor = A::Actor>,
{
    pub fn new(fut1: A, data: C) -> Chain<A, B, C> {
        Chain::First(fut1, Some(data))
    }

    pub fn poll<F>(
        mut self: Pin<&mut Self>,
        srv: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut Context<'_>,
        f: F,
    ) -> Poll<B::Output>
    where
        F: FnOnce(A::Output, C, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> B,
    {
        let mut f = Some(f);

        loop {
            let this = self.as_mut().project();
            let (output, data) = match this {
                ChainProj::First(fut1, data) => {
                    let output = match fut1.poll(srv, ctx, task) {
                        Poll::Ready(t) => t,
                        Poll::Pending => return Poll::Pending,
                    };
                    (output, data.take().unwrap())
                }
                ChainProj::Second(fut2) => {
                    return fut2.poll(srv, ctx, task);
                }
                ChainProj::Empty => unreachable!(),
            };

            self.set(Chain::Empty);
            let fut2 = (f.take().unwrap())(output, data, srv, ctx);
            self.set(Chain::Second(fut2))
        }
    }
}
