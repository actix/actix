use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::sync::oneshot::{Receiver as SyncReceiver};
use futures::unsync::oneshot::{Canceled, Receiver};

use actor::Actor;
use fut::ActorFuture;
use handler::{Handler, ResponseType};

enum RequestIo<M: ResponseType> {
    Local(Receiver<Result<M::Item, M::Error>>),
    Remote(SyncReceiver<Result<M::Item, M::Error>>),
}

/// `Request` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<A, M> where A: Actor, M: ResponseType {
    rx: RequestIo<M>,
    act: PhantomData<A>,
}

impl<A, M> Request<A, M> where A: Actor, M: ResponseType {

    pub(crate) fn local(rx: Receiver<Result<M::Item, M::Error>>) -> Request<A, M> {
        Request{rx: RequestIo::Local(rx), act: PhantomData}
    }

    pub(crate) fn remote(rx: SyncReceiver<Result<M::Item, M::Error>>) -> Request<A, M> {
        Request{rx: RequestIo::Remote(rx), act: PhantomData}
    }
}

impl<A, M> Future for Request<A, M> where A: Actor, M: ResponseType {
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.rx {
            RequestIo::Local(ref mut rx) => rx.poll(),
            RequestIo::Remote(ref mut rx) => rx.poll(),
        }
    }
}

impl<A, M> ActorFuture for Request<A, M> where A: Actor, M: ResponseType {
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;
    type Actor = A;

    fn poll(&mut self,
            _: &mut A,
            _: &mut <Self::Actor as Actor>::Context) -> Poll<Self::Item, Self::Error> {
        match self.rx {
            RequestIo::Local(ref mut rx) => rx.poll(),
            RequestIo::Remote(ref mut rx) => rx.poll(),
        }
    }
}

/// `Response` represents asynchronous message handling process.
pub struct Response<A, M> where A: Actor, M: ResponseType {
    inner: Option<ResponseTypeItem<A, M>>,
}

pub trait IntoResponse<A, M> where A: Actor + Handler<M>, M: ResponseType {
    fn into_response(self) -> Response<A, M>;
}

impl<A, M, I, E> IntoResponse<A, M> for Result<I, E>
    where A: Handler<M>,
          M: ResponseType<Item=I, Error=E>,
{
    fn into_response(self) -> Response<A, M> {
        Response {inner: Some(ResponseTypeItem::Result(self))}
    }
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
