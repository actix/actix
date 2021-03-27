use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::future::Either;

use crate::actor::Actor;
use crate::fut::ActorFuture;

impl<A, B, Act> ActorFuture<Act> for Either<A, B>
where
    A: ActorFuture<Act>,
    B: ActorFuture<Act, Output = A::Output>,
    Act: Actor,
{
    type Output = A::Output;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut Act,
        ctx: &mut Act::Context,
        task: &mut Context<'_>,
    ) -> Poll<A::Output> {
        // SAFETY:
        //
        // Copied from futures_util::future::Either::project method.
        // This is used to expose this method to public.
        // It has the same safety as the private one.
        let this = unsafe {
            match self.get_unchecked_mut() {
                Either::Left(a) => Either::Left(Pin::new_unchecked(a)),
                Either::Right(b) => Either::Right(Pin::new_unchecked(b)),
            }
        };

        match this {
            Either::Left(left) => left.poll(act, ctx, task),
            Either::Right(right) => right.poll(act, ctx, task),
        }
    }
}
