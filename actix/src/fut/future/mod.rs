use std::{
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

pub use map::Map;
use pin_project_lite::pin_project;
pub use then::Then;
pub use timeout::Timeout;

use crate::actor::Actor;

mod either;
mod map;
pub mod result;
mod then;
mod timeout;

/// Trait for types which are a placeholder of a value that may become
/// available at some later point in time.
///
/// [`ActorFuture`] is very similar to a regular [`Future`], only with subsequent combinator
/// closures accepting the actor and its context, in addition to the result.
///
/// [`ActorFuture`] allows for use cases where future processing requires access to the actor or
/// its context.
///
/// Here is an example of a handler on a single actor, deferring work to another actor, and then
/// updating the initiating actor's state:
///
/// ```no_run
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
/// #    fn handle(&mut self, _msg: OtherMessage, _ctx: &mut Context<Self>) -> Self::Result {}
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
/// # struct DeferredWork {}
/// #
/// # #[derive(Message)]
/// # #[rtype(result = "()")]
/// # struct OtherMessage {}
///
/// impl Handler<DeferredWork> for OriginalActor {
///     // Notice the `Response` is an `ActorFuture`-ized version of `Self::Message::Result`.
///     type Result = ResponseActFuture<Self, DeferredWorkResult>;
///
///     fn handle(&mut self, _msg: DeferredWork, _ctx: &mut Context<Self>) -> Self::Result {
///         // this creates a `Future` representing the `.send` and subsequent
///         // `Result` from `other_actor`
///         let update_self = self.other_actor
///             .send(OtherMessage {})
///             // Wrap that future so chained handlers can access the actor
///             // (`self` in the synchronous code) as well as the context.
///             .into_actor(self)
///             // once the wrapped future resolves, update this actor's state
///             .map(|result, actor, _ctx| {
///                 match result {
///                     Ok(v) => {
///                         // update actor (self) state
///                         actor.inner_state.update_from(v);
///                         Ok(())
///                     },
///                     // Failed to send message to other_actor
///                     Err(_e) => Err(()),
///                 }
///             });
///
///         // box and return the wrapped future
///         Box::pin(update_self)
///     }
/// }
/// ```
///
/// See also [`WrapFuture::into_actor()`] which provides future conversion.
pub trait ActorFuture<A: Actor> {
    /// The type of value produced on completion.
    type Output;

    fn poll(
        self: Pin<&mut Self>,
        srv: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output>;
}

pub trait ActorFutureExt<A: Actor>: ActorFuture<A> {
    /// Map this future's result to a different type, returning a new future of
    /// the resulting type.
    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        F: FnOnce(Self::Output, &mut A, &mut A::Context) -> U,
        Self: Sized,
    {
        Map::new(self, f)
    }

    /// Chain on a computation for when a future finished, passing the result of
    /// the future to the provided closure `f`.
    fn then<F, Fut>(self, f: F) -> Then<Self, Fut, F>
    where
        F: FnOnce(Self::Output, &mut A, &mut A::Context) -> Fut,
        Fut: ActorFuture<A>,
        Self: Sized,
    {
        then::new(self, f)
    }

    /// Add timeout to futures chain.
    ///
    /// `Err(())` returned as a timeout error.
    fn timeout(self, timeout: Duration) -> Timeout<Self>
    where
        Self: Sized,
    {
        Timeout::new(self, timeout)
    }

    /// Wrap the future in a Box, pinning it.
    ///
    /// A shortcut for wrapping in [`Box::pin`].
    fn boxed_local(self) -> LocalBoxActorFuture<A, Self::Output>
    where
        Self: Sized + 'static,
    {
        Box::pin(self)
    }
}

impl<F, A> ActorFutureExt<A> for F
where
    F: ActorFuture<A>,
    A: Actor,
{
}

/// Type alias for a pinned box [`ActorFuture`] trait object.
pub type LocalBoxActorFuture<A, I> = Pin<Box<dyn ActorFuture<A, Output = I>>>;

impl<F, A> ActorFuture<A> for Box<F>
where
    F: ActorFuture<A> + Unpin + ?Sized,
    A: Actor,
{
    type Output = F::Output;

    fn poll(
        mut self: Pin<&mut Self>,
        srv: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        Pin::new(&mut **self.as_mut()).poll(srv, ctx, task)
    }
}

impl<P, A> ActorFuture<A> for Pin<P>
where
    P: Unpin + DerefMut,
    <P as Deref>::Target: ActorFuture<A>,
    A: Actor,
{
    type Output = <<P as Deref>::Target as ActorFuture<A>>::Output;

    fn poll(
        self: Pin<&mut Self>,
        srv: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        Pin::get_mut(self).as_mut().poll(srv, ctx, task)
    }
}

/// Helper trait that allows conversion of normal future into [`ActorFuture`]
pub trait WrapFuture<A>
where
    A: Actor,
{
    /// The future that this type can be converted into.
    type Future: ActorFuture<A>;

    #[deprecated(since = "0.11.0", note = "Please use WrapFuture::into_actor")]
    #[doc(hidden)]
    fn actfuture(self) -> Self::Future;

    /// Convert normal future to a [`ActorFuture`]
    fn into_actor(self, a: &A) -> Self::Future;
}

impl<F: Future, A: Actor> WrapFuture<A> for F {
    type Future = FutureWrap<F, A>;

    #[doc(hidden)]
    fn actfuture(self) -> Self::Future {
        wrap_future(self)
    }

    fn into_actor(self, _: &A) -> Self::Future {
        wrap_future(self)
    }
}

pin_project! {
    pub struct FutureWrap<F, A>
    where
        F: Future,
        A: Actor,
    {
        #[pin]
        fut: F,
        _act: PhantomData<A>
    }
}

/// Converts normal future into [`ActorFuture`], allowing its processing to
/// use the actor's state.
///
/// See the documentation for [`ActorFuture`] for a practical example involving both
/// [`wrap_future`] and [`ActorFuture`]
pub fn wrap_future<F, A>(f: F) -> FutureWrap<F, A>
where
    F: Future,
    A: Actor,
{
    FutureWrap {
        fut: f,
        _act: PhantomData,
    }
}

impl<F, A> ActorFuture<A> for FutureWrap<F, A>
where
    F: Future,
    A: Actor,
{
    type Output = F::Output;

    fn poll(
        self: Pin<&mut Self>,
        _: &mut A,
        _: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        self.project().fut.poll(task)
    }
}
