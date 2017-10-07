use std;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{Canceled, Receiver};
use futures::sync::oneshot::{Receiver as SyncReceiver};

use fut::ActorFuture;
use actor::{Actor, Handler, ResponseType};

enum RequestIo<M, A: Handler<M>> {
    Local(Receiver<Result<A::Item, A::Error>>),
    Remote(SyncReceiver<Result<A::Item, A::Error>>),
}

/// `Request` is a `Future` which represents asyncronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<A, B, M>
    where A: Actor + Handler<M>,
          B: Actor
{
    rx: RequestIo<M, A>,
    act: PhantomData<B>,
}

impl<A, B, M> Request<A, B, M>
    where A: Actor + Handler<M>, B: Actor
{
    pub(crate) fn local(rx: Receiver<Result<A::Item, A::Error>>) -> Request<A, B, M>
    {
        Request{rx: RequestIo::Local(rx), act: PhantomData}
    }
    pub(crate) fn remote(rx: SyncReceiver<Result<A::Item, A::Error>>) -> Request<A, B, M>
    {
        Request{rx: RequestIo::Remote(rx), act: PhantomData}
    }
}

impl<A, B, M> Future for Request<A, B, M>
    where A: Actor + Handler<M>, B: Actor
{
    type Item = Result<A::Item, A::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
    {
        match self.rx {
            RequestIo::Local(ref mut rx) => rx.poll(),
            RequestIo::Remote(ref mut rx) => rx.poll(),
        }
    }
}

impl<A, B, M> ActorFuture for Request<A, B, M>
    where A: Actor + Handler<M>,
          B: Actor,
{
    type Item = Result<A::Item, A::Error>;
    type Error = Canceled;
    type Actor = B;

    fn poll(&mut self,
            _: &mut B,
            _: &mut <Self::Actor as Actor>::Context) -> Poll<Self::Item, Self::Error>
    {
        match self.rx {
            RequestIo::Local(ref mut rx) => rx.poll(),
            RequestIo::Remote(ref mut rx) => rx.poll(),
        }
    }
}

/// `Response` represents asyncronous message handling process.
pub struct Response<A, M> where A: Actor + ResponseType<M>,
{
    inner: Option<ResponseTypeItem<A, M>>,
}

enum ResponseTypeItem<A, M> where A: Actor + ResponseType<M>
{
    Item(A::Item),
    Error(A::Error),
    Fut(Box<ActorFuture<Item=A::Item, Error=A::Error, Actor=A>>)
}

/// Helper trait that converts compatible `ActorFuture` type to `Response`.
impl<A, M, T> std::convert::From<T> for Response<A, M>
    where A: Actor + Handler<M>,
          T: ActorFuture<Item=A::Item, Error=A::Error, Actor=A> + Sized + 'static,
{
    fn from(fut: T) -> Response<A, M> {
        Response {inner: Some(ResponseTypeItem::Fut(Box::new(fut)))}
    }
}

impl<A, M> Response<A, M> where A: Actor + ResponseType<M>
{
    /// Create response
    pub fn reply(val: A::Item) -> Self {
        Response {inner: Some(ResponseTypeItem::Item(val))}
    }

    /// Create async response
    pub fn async_reply<T>(fut: T) -> Self
        where T: ActorFuture<Item=A::Item, Error=A::Error, Actor=A> + Sized + 'static
    {
        Response {inner: Some(ResponseTypeItem::Fut(Box::new(fut)))}
    }

    /// Create unit response
    pub fn empty() -> Self where A: ResponseType<M, Item=()> {
        Response {inner: Some(ResponseTypeItem::Item(()))}
    }

    /// Create error response
    pub fn error(err: A::Error) -> Self {
        Response {inner: Some(ResponseTypeItem::Error(err))}
    }

    pub(crate) fn result(&mut self) -> Option<Result<A::Item, A::Error>> {
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

    pub(crate) fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<A::Item, A::Error>
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
