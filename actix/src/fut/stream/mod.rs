use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures_core::stream::Stream;
use pin_project_lite::pin_project;

pub use collect::Collect;
pub use finish::Finish;
pub use fold::Fold;
pub use map::Map;
pub use skip_while::SkipWhile;
pub use take_while::TakeWhile;
pub use then::Then;
pub use timeout::Timeout;

use crate::actor::Actor;

use super::future::ActorFuture;

mod collect;
mod finish;
mod fold;
mod map;
mod skip_while;
mod take_while;
mod then;
mod timeout;

/// A stream of values, not all of which may have been produced yet.
///
/// This is similar to `futures_util::stream::Stream` trait, except it works with `Actor`
pub trait ActorStream<A: Actor> {
    /// The type of item this stream will yield on success.
    type Item;

    fn poll_next(
        self: Pin<&mut Self>,
        srv: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>>;
}

pub trait ActorStreamExt<A: Actor>: ActorStream<A> {
    /// Maps this stream's items to a different type, returning a new stream of
    /// the resulting type.
    ///
    /// The provided closure is executed over all elements of this stream as
    /// they are made available. It is executed inline with calls to
    /// [`poll_next`](ActorStream::poll_next).
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it, similar to the existing `map` methods in the
    /// standard library.
    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        F: FnMut(Self::Item, &mut A, &mut A::Context) -> U,
        Self: Sized,
    {
        map::new(self, f)
    }

    /// Computes from this stream's items new items of a different type using
    /// an asynchronous closure.
    ///
    /// The provided closure `f` will be called with an `Item` once a value is
    /// ready, it returns a future which will then be run to completion
    /// to produce the next value on this stream.
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it.
    fn then<F, Fut>(self, f: F) -> Then<Self, F, Fut>
    where
        F: FnMut(Self::Item, &mut A, &mut A::Context) -> Fut,
        Fut: ActorFuture<A>,
        Self: Sized,
    {
        then::new(self, f)
    }

    /// Execute an accumulating asynchronous computation over a stream,
    /// collecting all the values into one final result.
    ///
    /// This combinator will accumulate all values returned by this stream
    /// according to the closure provided. The initial state is also provided to
    /// this method and then is returned again by each execution of the closure.
    /// Once the entire stream has been exhausted the returned future will
    /// resolve to this value.
    fn fold<F, Fut>(self, init: Fut::Output, f: F) -> Fold<Self, F, Fut, Fut::Output>
    where
        F: FnMut(Fut::Output, Self::Item, &mut A, &mut A::Context) -> Fut,
        Fut: ActorFuture<A>,
        Self: Sized,
    {
        fold::new(self, f, init)
    }

    /// Take elements from this stream while the provided asynchronous predicate
    /// resolves to `true`.
    ///
    /// This function, like `Iterator::take_while`, will take elements from the
    /// stream until the predicate `f` resolves to `false`. Once one element
    /// returns `false`, it will always return that the stream is done.
    fn take_while<F, Fut>(self, f: F) -> TakeWhile<Self, Self::Item, F, Fut>
    where
        F: FnMut(&Self::Item, &mut A, &mut A::Context) -> Fut,
        Fut: ActorFuture<A, Output = bool>,
        Self: Sized,
    {
        take_while::new(self, f)
    }

    /// Skip elements on this stream while the provided asynchronous predicate
    /// resolves to `true`.
    ///
    /// This function, like `Iterator::skip_while`, will skip elements on the
    /// stream until the predicate `f` resolves to `false`. Once one element
    /// returns `false`, all future elements will be returned from the underlying
    /// stream.
    fn skip_while<F, Fut>(self, f: F) -> SkipWhile<Self, Self::Item, F, Fut>
    where
        F: FnMut(&Self::Item, &mut A, &mut A::Context) -> Fut,
        Fut: ActorFuture<A, Output = bool>,
        Self: Sized,
    {
        skip_while::new(self, f)
    }

    /// Add timeout to stream.
    ///
    /// `Err(())` returned as a timeout error.
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where
        Self: Sized,
    {
        Timeout::new(self, timeout)
    }

    /// Transforms a stream into a collection, returning a
    /// future representing the result of that computation.
    ///
    /// The returned future will be resolved when the stream terminates.
    fn collect<C>(self) -> Collect<Self, C>
    where
        C: Default + Extend<Self::Item>,
        Self: Sized,
    {
        Collect::new(self)
    }

    /// Transforms a stream to a future that resolves when stream finishes.
    fn finish(self) -> Finish<Self>
    where
        Self: Sized,
    {
        Finish::new(self)
    }
}

impl<A, S> ActorStreamExt<A> for S
where
    S: ActorStream<A>,
    A: Actor,
{
}

/// Helper trait that allows conversion of normal stream into `ActorStream`
pub trait WrapStream<A>
where
    A: Actor,
{
    /// The stream that this type can be converted into.
    type Stream: ActorStream<A>;

    #[deprecated(since = "0.11.0", note = "Please use WrapStream::into_actor")]
    #[doc(hidden)]
    fn actstream(self) -> Self::Stream;

    /// Convert normal stream to a [`ActorStream`]
    fn into_actor(self, a: &A) -> Self::Stream;
}

impl<S, A> WrapStream<A> for S
where
    S: Stream,
    A: Actor,
{
    type Stream = StreamWrap<S, A>;

    #[doc(hidden)]
    fn actstream(self) -> Self::Stream {
        wrap_stream(self)
    }

    fn into_actor(self, _: &A) -> Self::Stream {
        wrap_stream(self)
    }
}

pin_project! {
    pub struct StreamWrap<S, A>
    where
        S: Stream,
        A: Actor
    {
        #[pin]
        stream: S,
        _act: PhantomData<A>
    }
}

/// Converts normal stream into `ActorStream`
pub fn wrap_stream<S, A>(stream: S) -> StreamWrap<S, A>
where
    S: Stream,
    A: Actor,
{
    StreamWrap {
        stream,
        _act: PhantomData,
    }
}

impl<S, A> ActorStream<A> for StreamWrap<S, A>
where
    S: Stream,
    A: Actor,
{
    type Item = S::Item;

    fn poll_next(
        self: Pin<&mut Self>,
        _: &mut A,
        _: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.project().stream.poll_next(task)
    }
}
