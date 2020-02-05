use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::channel::oneshot::Sender as SyncSender;

use crate::actor::{Actor, AsyncContext};
use crate::address::Addr;
use crate::context::Context;
use crate::fut::ActorFuture;
use crate::WrapFuture;

/// Describes how to handle messages of a specific type.
///
/// Implementing `Handler` is a general way to handle incoming
/// messages, streams, and futures.
///
/// The type `M` is a message which can be handled by the actor.
#[allow(unused_variables)]
pub trait Handler<M>
where
    Self: Actor,
    M: Message,
{
    /// The type of value that this handler will return.
    ///
    /// Check the [MessageResponse] trait for some details
    /// on how a message can be responded to.
    type Result: MessageResponse<Self, M>;

    /// This method is called for every message received by this actor.
    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> Self::Result;
}

/// Represent message that can be handled by an actor.
pub trait Message {
    /// The type of value that this message will resolved with if it is
    /// successful.
    type Result: 'static;
}

/// Allow users to use `Arc<M>` as a message without having to re-impl `Message`
impl<M, R: 'static> Message for Arc<M>
where
    M: Message<Result = R>,
{
    type Result = R;
}

/// Allow users to use `Box<M>` as a message without having to re-impl `Message`
impl<M, R: 'static> Message for Box<M>
where
    M: Message<Result = R>,
{
    type Result = R;
}

/// A helper type that implements the `MessageResponse` trait.
///
/// # Examples
/// ```rust, no_run
/// use actix::prelude::*;
///
/// #[derive(Message)]
/// #[rtype(result = "Response")]
/// struct Msg;
///
/// struct Response;
///
/// struct MyActor;
///
/// impl Actor for MyActor {
///     type Context = Context<Self>;
/// }
///
/// impl Handler<Msg> for MyActor {
///     type Result = MessageResult<Msg>;
///
///     fn handle(&mut self, _: Msg, _: &mut Context<Self>) -> Self::Result {
///         MessageResult(Response {})
///     }
/// }
/// ```
pub struct MessageResult<M: Message>(pub M::Result);

/// A specialized actor future for asynchronous message handling.
///
/// Intended be used from when the future returned will,
/// at some point, need to access Actor's internal state or context
/// in order to finish.
/// Check [ActorFuture] for available methods for accessing Actor's
/// internal state.
///
/// ## Note
/// It's important to keep in mind that the provided `AsyncContext`,
/// [Context], does not enforce the poll of any [ActorFuture] to be
/// exclusive. Therefore, if other instances of [ActorFuture] are spawned
/// into this Context **their execution won't necessarily be atomic**.
///
/// # Examples
/// ```rust, no_run
/// use actix::prelude::*;
///
/// #[derive(Message)]
/// #[rtype(result = "Result<usize, ()>")]
/// struct Msg;
///
/// struct MyActor;
///
/// impl Actor for MyActor {
///     type Context = Context<Self>;
/// }
///
/// impl Handler<Msg> for MyActor {
///     type Result = ResponseActFuture<Self, Result<usize, ()>>;
///
///     fn handle(&mut self, _: Msg, _: &mut Context<Self>) -> Self::Result {
///         Box::new(
///             async {
///                 // Some async computation
///                 42
///             }
///             .into_actor(self) // converts future to ActorFuture
///             .map(|res, _act, _ctx| {
///                 // Do some computation with actor's state or context
///                 Ok(res)
///             }),
///         )
///     }
/// }
/// ```
pub type ResponseActFuture<A, I> = Box<dyn ActorFuture<Output = I, Actor = A>>;

/// A specialized future for asynchronous message handling.
///
/// Intended be used for when the future returned doesn't
/// need to access Actor's internal state or context to progress, either
/// because it's completely agnostic to it, or because the required data has
/// already been moved inside the future and it won't need Actor state to continue.
///
/// # Examples
/// ```rust, no_run
/// use actix::prelude::*;
///
/// #[derive(Message)]
/// #[rtype(result = "Result<(), ()>")]
/// struct Msg;
///
/// struct MyActor;
///
/// impl Actor for MyActor {
///     type Context = Context<Self>;
/// }
///
/// impl Handler<Msg> for MyActor {
///     type Result = ResponseFuture<Result<(), ()>>;
///
///     fn handle(&mut self, _: Msg, _: &mut Context<Self>) -> Self::Result {
///         Box::pin(async move {
///             // Some async computation
///             Ok(())
///         })
///     }
/// }
/// ```
pub type ResponseFuture<I> = Pin<Box<dyn Future<Output = I>>>;

/// A trait that defines a message response channel.
pub trait ResponseChannel<M: Message>: 'static {
    fn is_canceled(&self) -> bool;

    fn send(self, response: M::Result);
}

/// A trait which defines message responses.
///
/// We offer implementation for some common language types, if you need
/// to respond with a new type you can use [MessageResult].
///
/// If `Actor::Context` implements [AsyncContext] it's possible to handle
/// the message asynchronously.
/// For asynchronous message handling we offer two possible response types
/// [ResponseFuture] and [ResponseActFuture].
/// - [ResponseFuture] should be used for when the future returned doesn't
///   need to access Actor's internal state or context to progress, either
///   because it's completely agnostic to it or because the required data has
///   already been moved to it and it won't need Actor state to continue.
/// - [ResponseActFuture] should be used from when the future returned
///   will, at some point, need to access Actor's internal state or context
///   in order to finish.
pub trait MessageResponse<A: Actor, M: Message> {
    fn handle<R: ResponseChannel<M>>(self, ctx: &mut A::Context, tx: Option<R>);
}

impl<M: Message + 'static> ResponseChannel<M> for SyncSender<M::Result>
where
    M::Result: Send,
{
    fn is_canceled(&self) -> bool {
        SyncSender::is_canceled(self)
    }

    fn send(self, response: M::Result) {
        let _ = Self::send(self, response);
    }
}

impl<M: Message + 'static> ResponseChannel<M> for () {
    fn is_canceled(&self) -> bool {
        true
    }
    fn send(self, _: M::Result) {}
}

impl<A, M> MessageResponse<A, M> for MessageResult<M>
where
    A: Actor,
    M: Message,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self.0);
        }
    }
}

impl<A, M, I: 'static, E: 'static> MessageResponse<A, M> for Result<I, E>
where
    A: Actor,
    M: Message<Result = Self>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

impl<A, M, I: 'static> MessageResponse<A, M> for Arc<I>
where
    A: Actor,
    M: Message<Result = Arc<I>>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

impl<A, M, I: 'static> MessageResponse<A, M> for Option<I>
where
    A: Actor,
    M: Message<Result = Self>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

impl<A, M, B> MessageResponse<A, M> for Addr<B>
where
    A: Actor,
    M: Message<Result = Self>,
    B: Actor<Context = Context<B>>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

impl<A, M, T: 'static> MessageResponse<A, M> for ResponseActFuture<A, T>
where
    A: Actor,
    M: Message<Result = T>,
    A::Context: AsyncContext<A>,
{
    fn handle<R: ResponseChannel<M>>(self, ctx: &mut A::Context, tx: Option<R>) {
        ctx.spawn(self.then(move |res, this, _| {
            if let Some(tx) = tx {
                tx.send(res);
            }
            async {}.into_actor(this)
        }));
    }
}

impl<A, M, I: 'static, E: 'static> MessageResponse<A, M> for ResponseFuture<Result<I, E>>
where
    A: Actor,
    M::Result: Send,
    M: Message<Result = Result<I, E>>,
    A::Context: AsyncContext<A>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        actix_rt::spawn(async move {
            let res = self.await;
            if let Some(tx) = tx {
                tx.send(res)
            }
        });
    }
}

enum ResponseTypeItem<I, E> {
    Result(Result<I, E>),
    Fut(Box<dyn Future<Output = Result<I, E>> + Unpin>),
}

/// Helper type for representing different type of message responses
pub struct Response<I, E> {
    item: ResponseTypeItem<I, E>,
}

impl<I, E> fmt::Debug for Response<I, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt = fmt.debug_struct("Response");
        match self.item {
            ResponseTypeItem::Result(_) => fmt.field("item", &"Result(_)".to_string()),
            ResponseTypeItem::Fut(_) => fmt.field("item", &"Fut(_)".to_string()),
        }
        .finish()
    }
}

impl<I, E> Response<I, E> {
    /// Creates an asynchronous response.
    pub fn fut<T>(fut: T) -> Self
    where
        T: Future<Output = Result<I, E>> + Unpin + 'static,
    {
        Self {
            item: ResponseTypeItem::Fut(Box::new(fut)),
        }
    }

    /// Creates a response.
    pub fn reply(val: Result<I, E>) -> Self {
        Self {
            item: ResponseTypeItem::Result(val),
        }
    }
}

impl<A, M, I: 'static, E: 'static> MessageResponse<A, M> for Response<I, E>
where
    A: Actor,
    M: Message<Result = Result<I, E>>,
    A::Context: AsyncContext<A>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        match self.item {
            ResponseTypeItem::Fut(fut) => {
                actix_rt::spawn(async move {
                    if let Some(tx) = tx {
                        tx.send(fut.await);
                    }
                });
            }
            ResponseTypeItem::Result(res) => {
                if let Some(tx) = tx {
                    tx.send(res);
                }
            }
        }
    }
}

enum ActorResponseTypeItem<A, I, E> {
    Result(Result<I, E>),
    Fut(Box<dyn ActorFuture<Output = Result<I, E>, Actor = A>>),
}

/// A helper type for representing different types of message responses.
pub struct ActorResponse<A, I, E> {
    item: ActorResponseTypeItem<A, I, E>,
}

impl<A, I, E> fmt::Debug for ActorResponse<A, I, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt = fmt.debug_struct("ActorResponse");
        match self.item {
            ActorResponseTypeItem::Result(_) => {
                fmt.field("item", &"Result(_)".to_string())
            }
            ActorResponseTypeItem::Fut(_) => fmt.field("item", &"Fut(_)".to_string()),
        }
        .finish()
    }
}

impl<A: Actor, I, E> ActorResponse<A, I, E> {
    /// Creates a response.
    pub fn reply(val: Result<I, E>) -> Self {
        Self {
            item: ActorResponseTypeItem::Result(val),
        }
    }

    /// Creates an asynchronous response.
    pub fn r#async<T>(fut: T) -> Self
    where
        T: ActorFuture<Output = Result<I, E>, Actor = A> + 'static,
    {
        Self {
            item: ActorResponseTypeItem::Fut(Box::new(fut)),
        }
    }
}

impl<A, M, I: 'static, E: 'static> MessageResponse<A, M> for ActorResponse<A, I, E>
where
    A: Actor,
    M: Message<Result = Result<I, E>>,
    A::Context: AsyncContext<A>,
{
    fn handle<R: ResponseChannel<M>>(self, ctx: &mut A::Context, tx: Option<R>) {
        match self.item {
            ActorResponseTypeItem::Fut(fut) => {
                let fut = fut.then(move |res, this, _| {
                    if let Some(tx) = tx {
                        tx.send(res)
                    }
                    async {}.into_actor(this)
                });

                ctx.spawn(fut);
            }
            ActorResponseTypeItem::Result(res) => {
                if let Some(tx) = tx {
                    tx.send(res);
                }
            }
        }
    }
}

macro_rules! SIMPLE_RESULT {
    ($type:ty) => {
        impl<A, M> MessageResponse<A, M> for $type
        where
            A: Actor,
            M: Message<Result = $type>,
        {
            fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
                if let Some(tx) = tx {
                    tx.send(self);
                }
            }
        }
    };
}

SIMPLE_RESULT!(());
SIMPLE_RESULT!(u8);
SIMPLE_RESULT!(u16);
SIMPLE_RESULT!(u32);
SIMPLE_RESULT!(u64);
SIMPLE_RESULT!(usize);
SIMPLE_RESULT!(i8);
SIMPLE_RESULT!(i16);
SIMPLE_RESULT!(i32);
SIMPLE_RESULT!(i64);
SIMPLE_RESULT!(isize);
SIMPLE_RESULT!(f32);
SIMPLE_RESULT!(f64);
SIMPLE_RESULT!(String);
SIMPLE_RESULT!(bool);
