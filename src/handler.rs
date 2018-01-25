use futures::{Async, Poll};

use actor::Actor;
use address::Address;
use fut::ActorFuture;
use context::Context;

/// Message response type
pub trait ResponseType {

    /// The type of value that this message will resolved with if it is successful.
    type Item;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error;
}

impl<I, E> ResponseType for Result<I, E> where I: ResponseType {
    type Item = <I as ResponseType>::Item;
    type Error = ();
}

pub type MessageResult<M: ResponseType> = Result<M::Item, M::Error>;

pub trait IntoResponse<A: Actor, M: ResponseType> {
    fn into_response(self) -> Response<A, M>;
}

impl<A, M> IntoResponse<A, M> for ()
    where A: Actor + Handler<M>, M: ResponseType<Item=(), Error=()>,
{
    fn into_response(self) -> Response<A, M> {
        Response::reply(Ok(()))
    }
}

impl<A, M> IntoResponse<A, M> for Response<A, M>
    where A: Actor + Handler<M>, M: ResponseType,
{
    fn into_response(self) -> Response<A, M> {
        self
    }
}

impl<A, M> IntoResponse<A, M> for Result<M::Item, M::Error>
    where A: Actor + Handler<M>, M: ResponseType,
{
    fn into_response(self) -> Response<A, M> {
        Response {inner: Some(ResponseTypeItem::Result(self))}
    }
}

impl<A, M, B> IntoResponse<A, M> for Address<B>
    where A: Actor + Handler<M>, M: ResponseType<Item=Address<B>, Error=()>,
          B: Actor<Context=Context<B>>
{
    fn into_response(self) -> Response<A, M> {
        Response::reply(Ok(self))
    }
}

pub type ResponseFuture<A, M: ResponseType> =
    Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>;

impl<A, M> IntoResponse<A, M> for Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>
    where A: Actor + Handler<M>, M: ResponseType
{
    fn into_response(self) -> Response<A, M> {
        self.into()
    }
}

/// Message handler
///
/// `Handler` implementation is a general way how to handle
/// incoming messages, streams, futures.
///
/// `M` is a message which can be handled by the actor.
#[allow(unused_variables)]
pub trait Handler<M> where Self: Actor, M: ResponseType
{
    type Result: IntoResponse<Self, M>;

    /// Method is called for every message received by this Actor
    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> Self::Result;
}


/// `Response` represents asynchronous message handling process.
pub struct Response<A, M> where A: Actor, M: ResponseType {
    inner: Option<ResponseTypeItem<A, M>>,
}

enum ResponseTypeItem<A, M> where A: Actor, M: ResponseType {
    Result(Result<M::Item, M::Error>),
    Fut(Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>)
}

/// Helper trait that converts compatible `ActorFuture` type to `Response`.
impl<A, M, T> From<T> for Response<A, M>
    where A: Actor + Handler<M>,
          M: ResponseType,
          T: ActorFuture<Item=M::Item, Error=M::Error, Actor=A> + Sized + 'static,
{
    fn from(fut: T) -> Response<A, M> {
        Response {inner: Some(ResponseTypeItem::Fut(Box::new(fut)))}
    }
}

impl<A, M, I, E> From<Result<I, E>> for Response<A, M>
    where A: Handler<M>,
          M: ResponseType<Item=I, Error=E>,
{
    fn from(res: Result<I, E>) -> Response<A, M> {
        Response {inner: Some(ResponseTypeItem::Result(res))}
    }
}

impl<A, M> From<Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>> for Response<A, M>
    where A: Handler<M>, M: ResponseType
{
    fn from(f: Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>) -> Response<A, M> {
        Response {inner: Some(ResponseTypeItem::Fut(f))}
    }
}

impl<A, M> Response<A, M> where A: Actor, M: ResponseType
{
    /// Create response
    pub fn reply(val: Result<M::Item, M::Error>) -> Self {
        Response {inner: Some(ResponseTypeItem::Result(val))}
    }

    /// Create async response
    pub fn async_reply<T>(fut: T) -> Self
        where T: ActorFuture<Item=M::Item, Error=M::Error, Actor=A> + 'static
    {
        Response {inner: Some(ResponseTypeItem::Fut(Box::new(fut)))}
    }

    pub(crate) fn result(&mut self) -> Option<Result<M::Item, M::Error>> {
        if let Some(item) = self.inner.take() {
            match item {
                ResponseTypeItem::Result(item) => Some(item),
                _ => None,
            }
        } else {
            None
        }
    }

    pub(crate) fn is_async(&self) -> bool {
        match self.inner {
            Some(ResponseTypeItem::Fut(_)) => true,
            _ => false,
        }
    }

    #[doc(hidden)]
    pub fn poll_response(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<M::Item, M::Error> {
        if let Some(item) = self.inner.take() {
            match item {
                ResponseTypeItem::Fut(mut fut) => {
                    match fut.poll(act, ctx) {
                        Ok(Async::NotReady) => {
                            self.inner = Some(ResponseTypeItem::Fut(fut));
                            Ok(Async::NotReady)
                        }
                        result => result
                    }
                },
                ResponseTypeItem::Result(result) => match result {
                    Ok(item) => Ok(Async::Ready(item)),
                    Err(err) => Err(err),
                },
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}
