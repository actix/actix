use futures::task::Poll;

use crate::actor::Actor;
use crate::fut::ActorFuture;

/// Future for the `map_err` combinator, changing the error type of a future.
///
/// This is created by the `Future::map_err` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct MapErr<A, F>
where
    A: ActorFuture,
{
    future: A,
    f: Option<F>,
}
/*
pub fn new<A, F>(future: A, f: F) -> MapErr<A, F>
where
    A: ActorFuture,
{
    MapErr { future, f: Some(f) }
}
impl<U, A, F> ActorFuture for MapErr<A, F>
where
    A: ActorFuture,
    F: FnOnce(A::Error, &mut A::Actor, &mut <A::Actor as Actor>::Context) -> U,
{
    type Item = A::Item;
    type Actor = A::Actor;
    fn poll(
        &mut self,
        act: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
    ) -> Poll<A::Item, U> {
        let e = match self.future.poll(act, ctx) {
            Ok(Poll::Pending) => return Ok(Poll::Pending),
            other => other,
        };
        match e {
            Err(e) => Err(self.f.take().expect("cannot poll MapErr twice")(
                e, act, ctx,
            )),
            Ok(err) => Ok(err),
        }
    }
}
*/

pub struct DropErr<A>
where
    A: ActorFuture,
{
    future: A,
}

/*
impl<A> DropErr<A>
where
    A: ActorFuture,
{
    pub(crate) fn new(future: A) -> DropErr<A> {
        DropErr { future }
    }
}
impl<A> ActorFuture for DropErr<A>
where
    A: ActorFuture,
{
    type Item = A::Item;
    type Actor = A::Actor;
    fn poll(
        &mut self,
        act: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
    ) -> Poll<A::Item, ()> {
        match self.future.poll(act, ctx) {
            Ok(Poll::Ready(item)) => Ok(Poll::Ready(item)),
            Ok(Poll::Pending) => Ok(Poll::Pending),
            Err(_) => Err(()),
        }
    }
}
*/
