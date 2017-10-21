use std;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{Canceled, Receiver};
use futures::sync::oneshot::{Receiver as SyncReceiver};

use fut::ActorFuture;
use actor::{Actor, Handler, ResponseType};

enum RequestIo<M: ResponseType> {
    Local(Receiver<Result<M::Item, M::Error>>),
    Remote(SyncReceiver<Result<M::Item, M::Error>>),
}

/// `Request` is a `Future` which represents asyncronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<A, M>
    where A: Actor,
          M: ResponseType,
{
    rx: RequestIo<M>,
    act: PhantomData<A>,
}

impl<A, M> Request<A, M>
    where A: Actor,
          M: ResponseType,
{
    pub(crate) fn local(rx: Receiver<Result<M::Item, M::Error>>) -> Request<A, M>
    {
        Request{rx: RequestIo::Local(rx), act: PhantomData}
    }
    pub(crate) fn remote(rx: SyncReceiver<Result<M::Item, M::Error>>) -> Request<A, M>
    {
        Request{rx: RequestIo::Remote(rx), act: PhantomData}
    }
}

impl<A, M> Future for Request<A, M>
    where A: Actor,
          M: ResponseType,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
    {
        match self.rx {
            RequestIo::Local(ref mut rx) => rx.poll(),
            RequestIo::Remote(ref mut rx) => rx.poll(),
        }
    }
}

impl<A, M> ActorFuture for Request<A, M>
    where A: Actor,
          M: ResponseType,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;
    type Actor = A;

    fn poll(&mut self,
            _: &mut A,
            _: &mut <Self::Actor as Actor>::Context) -> Poll<Self::Item, Self::Error>
    {
        match self.rx {
            RequestIo::Local(ref mut rx) => rx.poll(),
            RequestIo::Remote(ref mut rx) => rx.poll(),
        }
    }
}

/// `Response` represents asyncronous message handling process.
pub struct Response<A, M> where A: Actor, M: ResponseType,
{
    inner: Option<ResponseTypeItem<A, M>>,
}

enum ResponseTypeItem<A, M> where A: Actor, M: ResponseType
{
    Item(M::Item),
    Error(M::Error),
    Fut(Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>)
}

/// Helper trait that converts compatible `ActorFuture` type to `Response`.
impl<A, M, T> std::convert::From<T> for Response<A, M>
    where A: Actor + Handler<M>,
          M: ResponseType,
          T: ActorFuture<Item=M::Item, Error=M::Error, Actor=A> + Sized + 'static,
{
    fn from(fut: T) -> Response<A, M> {
        Response {inner: Some(ResponseTypeItem::Fut(Box::new(fut)))}
    }
}

impl<A, M> Response<A, M> where A: Actor, M: ResponseType
{
    /// Create response
    pub fn reply(val: M::Item) -> Self {
        Response {inner: Some(ResponseTypeItem::Item(val))}
    }

    /// Create async response
    pub fn async_reply<T>(fut: T) -> Self
        where T: ActorFuture<Item=M::Item, Error=M::Error, Actor=A> + Sized + 'static
    {
        Response {inner: Some(ResponseTypeItem::Fut(Box::new(fut)))}
    }

    /// Create unit response
    pub fn empty() -> Self where M: ResponseType<Item=()> {
        Response {inner: Some(ResponseTypeItem::Item(()))}
    }

    /// Create error response
    pub fn error(err: M::Error) -> Self {
        Response {inner: Some(ResponseTypeItem::Error(err))}
    }

    pub(crate) fn result(&mut self) -> Option<Result<M::Item, M::Error>> {
        if let Some(item) = self.inner.take() {
            match item {
                ResponseTypeItem::Item(item) => Some(Ok(item)),
                ResponseTypeItem::Error(err) => Some(Err(err)),
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

    pub(crate) fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<M::Item, M::Error>
    {
        if let Some(item) = self.inner.take() {
            match item {
                ResponseTypeItem::Fut(mut fut) => {
                    match fut.poll(act, ctx) {
                        Ok(Async::NotReady) => {
                            self.inner = Some(ResponseTypeItem::Fut(fut));
                            return Ok(Async::NotReady)
                        }
                        result => return result
                    }
                }
                ResponseTypeItem::Item(item) => return Ok(Async::Ready(item)),
                ResponseTypeItem::Error(err) => return Err(err)
            }
        }
        Ok(Async::NotReady)
    }
}
