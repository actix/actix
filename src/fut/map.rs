use std::pin::Pin;

use futures::{
    task::{Context, Poll},
    Future,
};
use pin_project::pin_project;

use crate::actor::Actor;
use crate::fut::ActorFuture;

/// Future for the `map` combinator, changing the type of a future.
///
/// This is created by the `ActorFuture::map` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Map<A, F>
where
    A: ActorFuture + Unpin,
{
    #[pin]
    future: A,
    f: Option<F>,
}

pub fn new<A, F>(future: A, f: F) -> Map<A, F>
where
    A: ActorFuture + Unpin,
{
    Map { future, f: Some(f) }
}

impl<U, A, F> ActorFuture for Map<A, F>
where
    A: ActorFuture + Unpin,
    F: FnOnce(A::Item, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> U,
{
    type Item = U;
    type Actor = A::Actor;
    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Item> {
        let mut this = self.as_mut();
        let e = match Pin::new(&mut this.future).poll(act, ctx, task) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(e) => e,
        };
        // match e {
        //     Ok(item) => Poll::Ready(self.f.take().expect("cannot poll Map twice")(
        //         item, act, ctx,
        //     )),
        //     Err(err) => Poll::Ready(Err(err)),
        // }
        Poll::Ready(self.f.take().expect("cannot poll Map twice")(e, act, ctx))
    }
}
