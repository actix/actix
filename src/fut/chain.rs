use std::pin::Pin;
use std::task::{Context, Poll};

use crate::actor::Actor;
use crate::fut::ActorFuture;

#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub enum Chain<A, B, C> {
    First(A, Option<C>),
    Second(B),
    Empty,
}

impl<A: Unpin, B: Unpin, C> Unpin for Chain<A, B, C> {}

impl<A, B, C> Chain<A, B, C>
where
    A: ActorFuture,
    B: ActorFuture<Actor = A::Actor>,
{
    pub fn new(fut1: A, data: C) -> Chain<A, B, C> {
        Chain::First(fut1, Some(data))
    }

    pub fn poll<F>(
        self: Pin<&mut Self>,
        srv: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut Context<'_>,
        f: F,
    ) -> Poll<B::Output>
    where
        F: FnOnce(A::Output, C, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> B,
    {
        let mut f = Some(f);

        // Safe to call `get_unchecked_mut` because we won't move the futures.
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            let (output, data) = match this {
                Chain::First(fut1, data) => {
                    let output =
                        match unsafe { Pin::new_unchecked(fut1) }.poll(srv, ctx, task) {
                            Poll::Ready(t) => t,
                            Poll::Pending => return Poll::Pending,
                        };
                    (output, data.take().unwrap())
                }
                Chain::Second(fut2) => {
                    return unsafe { Pin::new_unchecked(fut2) }.poll(srv, ctx, task);
                }
                Chain::Empty => unreachable!(),
            };

            *this = Chain::Empty; // Drop fut1
            let fut2 = (f.take().unwrap())(output, data, srv, ctx);
            *this = Chain::Second(fut2)
        }
    }
}
