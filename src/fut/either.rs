use std::pin::Pin;
use std::task::{Context, Poll};

use crate::actor::Actor;
use crate::fut::ActorFuture;
use pin_project::pin_project;

/// Combines two different futures yielding the same item and error
/// types into a single type.
#[pin_project(project = EitherProj)]
#[derive(Debug)]
pub enum Either<A, B> {
    /// First branch of the type
    Left(#[pin] A),
    /// Second branch of the type
    Right(#[pin] B),
}

impl<A, B, T> Either<(T, A), (T, B)> {
    /// Factor out a homogeneous type from an either of pairs.
    ///
    /// Here, the homogeneous type is the first element of the pairs.
    pub fn factor_first(self) -> (T, Either<A, B>) {
        match self {
            Either::Left((x, a)) => (x, Either::Left(a)),
            Either::Right((x, b)) => (x, Either::Right(b)),
        }
    }
}

impl<A, B, T> Either<(A, T), (B, T)> {
    /// Factor out a homogeneous type from an either of pairs.
    ///
    /// Here, the homogeneous type is the second element of the pairs.
    pub fn factor_second(self) -> (Either<A, B>, T) {
        match self {
            Either::Left((a, x)) => (Either::Left(a), x),
            Either::Right((b, x)) => (Either::Right(b), x),
        }
    }
}

impl<T> Either<T, T> {
    /// Extract the value of an either over two equivalent types.
    pub fn into_inner(self) -> T {
        match self {
            Either::Left(x) => x,
            Either::Right(x) => x,
        }
    }
}

impl<A, B> ActorFuture for Either<A, B>
where
    A: ActorFuture,
    B: ActorFuture<Output = A::Output, Actor = A::Actor>,
{
    type Output = A::Output;
    type Actor = A::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<A::Output> {
        match self.project() {
            EitherProj::Left(x) => x.poll(act, ctx, task),
            EitherProj::Right(x) => x.poll(act, ctx, task),
        }
    }
}
