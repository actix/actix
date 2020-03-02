//! Custom `Future` implementation with `Actix` support

use std::marker::PhantomData;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::{future::Future, stream::Stream};
use pin_project::pin_project;

mod chain;
mod either;
mod helpers;
mod map;
mod ready_fut;
mod result;
mod stream_finish;
mod stream_fold;
mod stream_map;
mod stream_then;
mod stream_timeout;
mod then;
mod timeout;

pub use self::either::Either;
pub use self::helpers::{Finish, FinishStream};
pub use self::map::Map;
pub use self::ready_fut::{ready, Ready};
pub use self::result::{err, ok, result, FutureResult};
pub use self::stream_finish::StreamFinish;
pub use self::stream_fold::StreamFold;
pub use self::stream_map::StreamMap;
pub use self::stream_then::StreamThen;
pub use self::stream_timeout::StreamTimeout;
pub use self::then::Then;
pub use self::timeout::Timeout;

use crate::actor::Actor;
use std::pin::Pin;

/// Trait for types which are a placeholder of a value that may become
/// available at some later point in time.
///
/// `ActorFuture` is very similar to a regular `Future`, only with subsequent combinator closures accepting the actor and its context, in addition to the result.
///
/// `ActorFuture` allows for use cases where future processing requires access to the actor or its context.
///
/// Here is an example of a handler on a single actor, deferring work to another actor, and
/// then updating the initiating actor's state:
///
/// ```rust,no_run
/// use actix::prelude::*;
///
/// // The response type returned by the actor future
/// type OriginalActorResponse = ();
/// // The error type returned by the actor future
/// type MessageError = ();
/// // This is the needed result for the DeferredWork message
/// // It's a result that combine both Response and Error from the future response.
/// type DeferredWorkResult = Result<OriginalActorResponse, MessageError>;
/// #
/// # struct ActorState {}
/// #
/// # impl ActorState {
/// #    fn update_from(&mut self, _result: ()) {}
/// # }
/// #
/// # struct OtherActor {}
/// #
/// # impl Actor for OtherActor {
/// #    type Context = Context<Self>;
/// # }
/// #
/// # impl Handler<OtherMessage> for OtherActor {
/// #    type Result = ();
/// #
/// #    fn handle(&mut self, _msg: OtherMessage, _ctx: &mut Context<Self>) -> Self::Result {
/// #    }
/// # }
/// #
/// # struct OriginalActor{
/// #     other_actor: Addr<OtherActor>,
/// #     inner_state: ActorState
/// # }
/// #
/// # impl Actor for OriginalActor{
/// #     type Context = Context<Self>;
/// # }
/// #
/// # #[derive(Message)]
/// # #[rtype(result = "Result<(), MessageError>")]
/// # struct DeferredWork{}
/// #
/// # #[derive(Message)]
/// # #[rtype(result = "()")]
/// # struct OtherMessage{}
///
/// impl Handler<DeferredWork> for OriginalActor {
///     // Notice the `Response` is an `ActorFuture`-ized version of `Self::Message::Result`.
///     type Result = ResponseActFuture<Self, Result<OriginalActorResponse, MessageError>>;
///
///     fn handle(&mut self, _msg: DeferredWork, _ctx: &mut Context<Self>) -> Self::Result {
///         // this creates a `Future` representing the `.send` and subsequent `Result` from
///         // `other_actor`
///         let send_to_other = self.other_actor
///             .send(OtherMessage {});
///
///         // Wrap that `Future` so subsequent chained handlers can access
///         // the `actor` (`self` in the  synchronous code) as well as the context.
///         let send_to_other = actix::fut::wrap_future::<_, Self>(send_to_other);
///
///         // once the wrapped future resolves, update this actor's state
///         let update_self = send_to_other.map(|result, actor, _ctx| {
///             // Actor's state updated here
///             match result {
///                 Ok(v) => {
///                     actor.inner_state.update_from(v);
///                     Ok(())
///                 },
///                 // Failed to send message to other_actor
///                 Err(_e) => Err(()),
///             }
///         });
///
///         // return the wrapped future
///         Box::pin(update_self)
///     }
/// }
///
/// ```
///
/// See also [into_actor](trait.WrapFuture.html#tymethod.into_actor), which provides future conversion using trait
pub trait ActorFuture {
    /// The type of value that this future will resolved with if it is
    /// successful.
    type Output;

    /// The actor within which this future runs
    type Actor: Actor;

    fn poll(
        self: Pin<&mut Self>,
        srv: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output>;

    /// Map this future's result to a different type, returning a new future of
    /// the resulting type.
    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        F: FnOnce(
            Self::Output,
            &mut Self::Actor,
            &mut <Self::Actor as Actor>::Context,
        ) -> U,
        Self: Sized,
    {
        map::new(self, f)
    }

    /// Chain on a computation for when a future finished, passing the result of
    /// the future to the provided closure `f`.
    fn then<F, B>(self, f: F) -> Then<Self, B, F>
    where
        F: FnOnce(
            Self::Output,
            &mut Self::Actor,
            &mut <Self::Actor as Actor>::Context,
        ) -> B,
        B: IntoActorFuture<Actor = Self::Actor>,
        Self: Sized,
    {
        then::new(self, f)
    }

    /// Add timeout to futures chain.
    ///
    /// `err` value get returned as a timeout error.
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where
        Self: Sized,
    {
        timeout::new(self, timeout)
    }
}

/// A stream of values, not all of which may have been produced yet.
///
/// This is similar to `futures_util::stream::Stream` trait, except it works with `Actor`
pub trait ActorStream {
    /// The type of item this stream will yield on success.
    type Item;

    /// The actor within which this stream runs.
    type Actor: Actor;

    fn poll_next(
        self: Pin<&mut Self>,
        srv: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>>;

    /// Converts a stream of type `T` to a stream of type `U`.
    fn map<U, F>(self, f: F) -> StreamMap<Self, F>
    where
        F: FnMut(
            Self::Item,
            &mut Self::Actor,
            &mut <Self::Actor as Actor>::Context,
        ) -> U,
        Self: Sized,
    {
        stream_map::new(self, f)
    }

    /// Chain on a computation for when a value is ready, passing the resulting
    /// item to the provided closure `f`.
    fn then<F, U>(self, f: F) -> StreamThen<Self, F, U>
    where
        F: FnMut(
            Self::Item,
            &mut Self::Actor,
            &mut <Self::Actor as Actor>::Context,
        ) -> U,
        U: IntoActorFuture<Actor = Self::Actor>,
        Self: Unpin + Sized,
    {
        stream_then::new(self, f)
    }

    /// Execute an accumulating computation over a stream, collecting all the
    /// values into one final result.
    fn fold<F, T, Fut>(self, init: T, f: F) -> StreamFold<Self, F, Fut, T>
    where
        F: FnMut(
            T,
            Self::Item,
            &mut Self::Actor,
            &mut <Self::Actor as Actor>::Context,
        ) -> Fut,
        Fut: IntoActorFuture<Actor = Self::Actor, Output = T>,
        Self: Sized,
    {
        stream_fold::new(self, f, init)
    }

    /// Add timeout to stream.
    ///
    /// `err` value get returned as a timeout error.
    fn timeout(self, timeout: Duration) -> StreamTimeout<Self>
    where
        Self: Sized + Unpin,
    {
        stream_timeout::new(self, timeout)
    }

    /// Converts a stream to a future that resolves when stream finishes.
    fn finish(self) -> StreamFinish<Self>
    where
        Self: Sized + Unpin,
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
    #[rustfmt::skip]
    type Future: ActorFuture<Output=Self::Output, Actor=Self::Actor>;

    /// The item that the future may resolve with.
    type Output;
    /// The actor within which this future runs
    type Actor: Actor;

    /// Consumes this object and produces a future.
    fn into_future(self) -> Self::Future;
}

impl<F: ActorFuture> IntoActorFuture for F {
    type Future = F;
    type Output = F::Output;
    type Actor = F::Actor;

    fn into_future(self) -> F {
        self
    }
}

impl<F: ActorFuture + Unpin + ?Sized> ActorFuture for Box<F> {
    type Output = F::Output;
    type Actor = F::Actor;

    fn poll(
        mut self: Pin<&mut Self>,
        srv: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        Pin::new(&mut **self.as_mut()).poll(srv, ctx, task)
    }
}

impl<P> ActorFuture for Pin<P>
where
    P: Unpin + std::ops::DerefMut,
    <P as std::ops::Deref>::Target: ActorFuture,
{
    type Output = <<P as std::ops::Deref>::Target as ActorFuture>::Output;
    type Actor = <<P as std::ops::Deref>::Target as ActorFuture>::Actor;

    fn poll(
        self: Pin<&mut Self>,
        srv: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        Pin::get_mut(self).as_mut().poll(srv, ctx, task)
    }
}

/// Helper trait that allows conversion of normal future into `ActorFuture`
pub trait WrapFuture<A>
where
    A: Actor,
{
    /// The future that this type can be converted into.
    type Future: ActorFuture<Output = Self::Output, Actor = A>;

    /// The item that the future may resolve with.
    type Output;

    #[doc(hidden)]
    fn actfuture(self) -> Self::Future;

    /// Convert normal future to a ActorFuture
    fn into_actor(self, a: &A) -> Self::Future;
}

impl<F: Future, A: Actor> WrapFuture<A> for F {
    type Future = FutureWrap<F, A>;
    type Output = F::Output;

    #[doc(hidden)]
    fn actfuture(self) -> Self::Future {
        wrap_future(self)
    }

    fn into_actor(self, _: &A) -> Self::Future {
        wrap_future(self)
    }
}

#[pin_project]
pub struct FutureWrap<F, A>
where
    F: Future,
{
    #[pin]
    fut: F,
    act: PhantomData<A>,
}

/// Converts normal future into `ActorFuture`, allowing its processing to
/// use the actor's state.
///
/// See the documentation for [ActorFuture](trait.ActorFuture.html) for a practical example involving both
/// `wrap_future` and `ActorFuture`
pub fn wrap_future<F, A>(f: F) -> FutureWrap<F, A>
where
    F: Future,
{
    FutureWrap {
        fut: f,
        act: PhantomData,
    }
}

impl<F, A> ActorFuture for FutureWrap<F, A>
where
    F: Future,
    A: Actor,
{
    type Output = F::Output;
    type Actor = A;

    fn poll(
        self: Pin<&mut Self>,
        _: &mut Self::Actor,
        _: &mut <Self::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        self.project().fut.poll(task)
    }
}

/// Helper trait that allows conversion of normal stream into `ActorStream`
pub trait WrapStream<A>
where
    A: Actor,
{
    /// The stream that this type can be converted into.
    type Stream: ActorStream<Item = Self::Item, Actor = A>;

    /// The item that the future may resolve with.
    type Item;

    #[doc(hidden)]
    fn actstream(self) -> Self::Stream;

    /// Convert normal stream to a ActorStream
    fn into_actor(self, a: &A) -> Self::Stream;
}

impl<S: Stream + Unpin, A: Actor> WrapStream<A> for S {
    type Stream = StreamWrap<S, A>;
    type Item = S::Item;

    #[doc(hidden)]
    fn actstream(self) -> Self::Stream {
        wrap_stream(self)
    }

    fn into_actor(self, _: &A) -> Self::Stream {
        wrap_stream(self)
    }
}
#[pin_project]
pub struct StreamWrap<S, A>
where
    S: Stream,
{
    #[pin]
    st: S,
    act: PhantomData<A>,
}
/// Converts normal stream into `ActorStream`
pub fn wrap_stream<S, A>(s: S) -> StreamWrap<S, A>
where
    S: Stream,
{
    StreamWrap {
        st: s,
        act: PhantomData,
    }
}

impl<S, A> ActorStream for StreamWrap<S, A>
where
    S: Stream,
    A: Actor,
{
    type Item = S::Item;
    type Actor = A;

    fn poll_next(
        self: Pin<&mut Self>,
        _: &mut Self::Actor,
        _: &mut <Self::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.project().st.poll_next(task)
    }
}
