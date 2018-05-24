use futures::Poll;

use actor::Actor;
use fut::ActorFuture;

/// Combines two different futures yielding the same item and error
/// types into a single type.
#[derive(Debug)]
pub enum Either<A, B> {
    /// First branch of the type
    A(A),
    /// Second branch of the type
    B(B),
}

impl<T, A, B> Either<(T, A), (T, B)> {
    /// Splits out the homogeneous type from an either of tuples.
    ///
    /// This method is typically useful when combined with the `Future::select2`
    /// combinator.
    pub fn split(self) -> (T, Either<A, B>) {
        match self {
            Either::A((a, b)) => (a, Either::A(b)),
            Either::B((a, b)) => (a, Either::B(b)),
        }
    }
}

impl<A, B> ActorFuture for Either<A, B>
where
    A: ActorFuture,
    B: ActorFuture<Item = A::Item, Error = A::Error, Actor = A::Actor>,
{
    type Item = A::Item;
    type Error = A::Error;
    type Actor = A::Actor;

    fn poll(
        &mut self, act: &mut A::Actor, ctx: &mut <A::Actor as Actor>::Context,
    ) -> Poll<A::Item, B::Error> {
        match *self {
            Either::A(ref mut a) => a.poll(act, ctx),
            Either::B(ref mut b) => b.poll(act, ctx),
        }
    }
}
