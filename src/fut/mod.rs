//! Custom `Future` implementation with `Actix` support

use std::marker::PhantomData;
use futures::{Future, Poll, Stream};

mod chain;
mod and_then;
mod either;
mod result;
mod then;
mod map;
mod map_err;
mod stream_map;
mod stream_map_err;
mod stream_then;
mod stream_and_then;
mod stream_finish;

pub use self::either::Either;
pub use self::and_then::AndThen;
pub use self::then::Then;
pub use self::map::Map;
pub use self::map_err::{MapErr, DropErr};
pub use self::result::{result, ok, err, FutureResult};
pub use self::stream_map::StreamMap;
pub use self::stream_map_err::StreamMapErr;
pub use self::stream_then::StreamThen;
pub use self::stream_and_then::StreamAndThen;
pub use self::stream_finish::StreamFinish;

use actor::Actor;


/// Trait for types which are a placeholder of a value that may become
/// available at some later point in time.
///
/// This is similar to `futures::Future` trait, except it works with `Actor`
pub trait ActorFuture {

    /// The type of value that this future will resolved with if it is
    /// successful.
    type Item;

    /// The type of error that this future will resolve with if it fails in a
    /// normal fashion.
    type Error;

    /// The actor within which this future runs
    type Actor: Actor;

    fn poll(&mut self, srv: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context)
            -> Poll<Self::Item, Self::Error>;

    /// Map this future's result to a different type, returning a new future of
    /// the resulting type.
    fn map<F, U>(self, f: F) -> Map<Self, F>
        where F: FnOnce(Self::Item, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> U,
              Self: Sized,
    {
        map::new(self, f)
    }

    /// Map this future's error to a different error, returning a new future.
    fn map_err<F, E>(self, f: F) -> MapErr<Self, F>
        where F: FnOnce(Self::Error, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> E,
              Self: Sized,
    {
        map_err::new(self, f)
    }

    /// Drop this future's error, returning a new future.
    fn drop_err(self) -> DropErr<Self> where Self: Sized
    {
        map_err::DropErr::new(self)
    }

    /// Chain on a computation for when a future finished, passing the result of
    /// the future to the provided closure `f`.
    fn then<F, B>(self, f: F) -> Then<Self, B, F>
        where F: FnOnce(Result<Self::Item, Self::Error>,
                        &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> B,
              B: IntoActorFuture<Actor=Self::Actor>,
              Self: Sized,
    {
        then::new(self, f)
    }

    /// Execute another future after this one has resolved successfully.
    fn and_then<F, B>(self, f: F) -> AndThen<Self, B, F>
        where F: FnOnce(Self::Item, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> B,
              B: IntoActorFuture<Error=Self::Error, Actor=Self::Actor>,
              Self: Sized,
    {
        and_then::new(self, f)
    }

}

/// A stream of values, not all of which may have been produced yet.
///
/// This is similar to `futures::Stream` trait, except it works with `Actor`
pub trait ActorStream {

    /// The type of item this stream will yield on success.
    type Item;

    /// The type of error this stream may generate.
    type Error;

    /// The actor within which this stream runs.
    type Actor: Actor;

    fn poll(&mut self, srv: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context)
            -> Poll<Option<Self::Item>, Self::Error>;

    /// Converts a stream of type `T` to a stream of type `U`.
    fn map<U, F>(self, f: F) -> StreamMap<Self, F>
        where F: FnMut(Self::Item, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> U,
              Self: Sized,
    {
        stream_map::new(self, f)
    }

    /// Converts a stream of error type `T` to a stream of error type `E`.
    fn map_err<E, F>(self, f: F) -> StreamMapErr<Self, F>
        where F: FnMut(Self::Error, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> E,
              Self: Sized,
    {
        stream_map_err::new(self, f)
    }

    /// Chain on a computation for when a value is ready, passing the resulting
    /// item to the provided closure `f`.
    fn then<F, U>(self, f: F) -> StreamThen<Self, F, U>
        where F: FnMut(Result<Self::Item, Self::Error>,
                       &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> U,
              U: IntoActorFuture<Actor=Self::Actor>,
              Self: Sized,
    {
        stream_then::new(self, f)
    }

    /// Chain on a computation for when a value is ready, passing the successful
    /// results to the provided closure `f`.
    fn and_then<F, U>(self, f: F) -> StreamAndThen<Self, F, U>
        where F: FnMut(Self::Item, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> U,
              U: IntoActorFuture<Error=Self::Error, Actor=Self::Actor>,
              Self: Sized,
    {
        stream_and_then::new(self, f)
    }

    /// Converts a stream to a future that resolves when stream completes.
    fn finish(self) -> StreamFinish<Self>
        where Self: Sized
    {
        stream_finish::new(self)
    }
}


/// Class of types which can be converted into an actor future.
///
/// This trait is very similar to the `IntoIterator` trait and is intended to be
/// used in a very similar fashion.
pub trait IntoActorFuture {
    /// The future that this type can be converted into.
    type Future: ActorFuture<Item=Self::Item, Error=Self::Error, Actor=Self::Actor>;

    /// The item that the future may resolve with.
    type Item;
    /// The error that the future may resolve with.
    type Error;
    /// The actor within which this future runs
    type Actor: Actor;

    /// Consumes this object and produces a future.
    fn into_future(self) -> Self::Future;
}

impl<F: ActorFuture> IntoActorFuture for F {
    type Future = F;
    type Item = F::Item;
    type Error = F::Error;
    type Actor = F::Actor;

    fn into_future(self) -> F {
        self
    }
}

/// Helper trait that allows conversion of normal future into `ActorFuture`
pub trait WrapFuture<A> where A: Actor
{
    /// The future that this type can be converted into.
    type Future: ActorFuture<Item=Self::Item, Error=Self::Error, Actor=A>;

    /// The item that the future may resolve with.
    type Item;
    /// The error that the future may resolve with.
    type Error;

    fn actfuture(self) -> Self::Future;
}

impl<F: Future, A: Actor> WrapFuture<A> for F {
    type Future = FutureWrap<F, A>;
    type Item = F::Item;
    type Error = F::Error;

    fn actfuture(self) -> Self::Future {
        wrap_future(self)
    }
}

pub struct FutureWrap<F, A> where F: Future {
    fut: F,
    act: PhantomData<A>,
}

/// Converts normal future into `ActorFuture`
pub fn wrap_future<F, A>(f: F) -> FutureWrap<F, A>
    where F: Future
{
    FutureWrap{fut: f, act: PhantomData}
}

impl<F, A> ActorFuture for FutureWrap<F, A>
    where F: Future,
          A: Actor,
{
    type Item = F::Item;
    type Error = F::Error;
    type Actor = A;

    fn poll(&mut self, _: &mut Self::Actor, _: &mut <Self::Actor as Actor>::Context)
            -> Poll<Self::Item, Self::Error>
    {
        self.fut.poll()
    }
}

/// Helper trait that allows conversion of normal stream into `ActorStream`
pub trait WrapStream<A> where A: Actor
{
    /// The stream that this type can be converted into.
    type Stream: ActorStream<Item=Self::Item, Error=Self::Error, Actor=A>;

    /// The item that the future may resolve with.
    type Item;
    /// The error that the future may resolve with.
    type Error;

    fn actstream(self) -> Self::Stream;
}

impl<S: Stream, A: Actor> WrapStream<A> for S {
    type Stream = StreamWrap<S, A>;
    type Item = S::Item;
    type Error = S::Error;

    fn actstream(self) -> Self::Stream {
        wrap_stream(self)
    }
}

pub struct StreamWrap<S, A> where S: Stream {
    st: S,
    act: PhantomData<A>,
}

/// Converts normal stream into `ActorStream`
pub fn wrap_stream<S, A>(s: S) -> StreamWrap<S, A>
    where S: Stream
{
    StreamWrap{st: s, act: PhantomData}
}

impl<S, A> ActorStream for StreamWrap<S, A>
    where S: Stream,
          A: Actor,
{
    type Item = S::Item;
    type Error = S::Error;
    type Actor = A;

    fn poll(&mut self, _: &mut Self::Actor, _: &mut <Self::Actor as Actor>::Context)
            -> Poll<Option<Self::Item>, Self::Error>
    {
        self.st.poll()
    }
}
