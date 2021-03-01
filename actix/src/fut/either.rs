use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::ActorFuture;

pin_project! {
    /// Combines two different futures yielding the same item and error
    /// types into a single type.
    #[project = EitherProj]
    #[derive(Debug)]
    pub enum Either<A, B> {
        /// First branch of the type
        Left {
            #[pin]
            left: A
        },
        /// Second branch of the type
        Right {
            #[pin]
            right: B
        },
    }
}

impl<A, B> Either<A, B> {
    /// construct first branch of the type
    pub fn left(left: A) -> Self {
        Self::Left { left }
    }

    /// construct second branch of the type
    pub fn right(right: B) -> Self {
        Self::Right { right }
    }
}

impl<A, B, T> Either<(T, A), (T, B)> {
    /// Factor out a homogeneous type from an either of pairs.
    ///
    /// Here, the homogeneous type is the first element of the pairs.
    pub fn factor_left(self) -> (T, Either<A, B>) {
        match self {
            Either::Left { left: (x, left) } => (x, Either::Left { left }),
            Either::Right { right: (x, right) } => (x, Either::Right { right }),
        }
    }
}

impl<A, B, T> Either<(A, T), (B, T)> {
    /// Factor out a homogeneous type from an either of pairs.
    ///
    /// Here, the homogeneous type is the second element of the pairs.
    pub fn factor_right(self) -> (Either<A, B>, T) {
        match self {
            Either::Left { left: (left, x) } => (Either::Left { left }, x),
            Either::Right { right: (right, x) } => (Either::Right { right }, x),
        }
    }
}

impl<T> Either<T, T> {
    /// Extract the value of an either over two equivalent types.
    pub fn into_inner(self) -> T {
        match self {
            Either::Left { left } => left,
            Either::Right { right } => right,
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
            EitherProj::Left { left } => left.poll(act, ctx, task),
            EitherProj::Right { right } => right.poll(act, ctx, task),
        }
    }
}
