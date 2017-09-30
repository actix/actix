use std;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{Canceled, Receiver};
use futures::sync::oneshot::{Receiver as SyncReceiver};

use fut::ActorFuture;
use context::Context;
use actor::{Actor, MessageHandler, MessageResponse};

enum RequestIo<M, A: MessageHandler<M>> {
    Local(Receiver<Result<A::Item, A::Error>>),
    Remote(SyncReceiver<Result<A::Item, A::Error>>),
}

/// `Request` is a `Future` which represents asyncronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<A, B, M>
    where A: MessageHandler<M>,
          B: Actor
{
    rx: RequestIo<M, A>,
    act: PhantomData<B>,
}

impl<A, B, M> Request<A, B, M>
    where A: MessageHandler<M>, B: Actor
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
    where A: MessageHandler<M>, B: Actor
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
    where A: MessageHandler<M>,
          B: Actor,
{
    type Item = Result<A::Item, A::Error>;
    type Error = Canceled;
    type Actor = B;

    fn poll(&mut self, _: &mut B, _: &mut Context<B>) -> Poll<Self::Item, Self::Error>
    {
        match self.rx {
            RequestIo::Local(ref mut rx) => rx.poll(),
            RequestIo::Remote(ref mut rx) => rx.poll(),
        }
    }
}

enum ResponseTypeItem<A, M> where A: Actor + MessageResponse<M>
{
    Item(A::Item),
    Error(A::Error),
    Fut(Box<ActorFuture<Item=A::Item, Error=A::Error, Actor=A>>)
}

/// `Response` represents asyncronous message handling process.
pub struct Response<A, M> where A: Actor + MessageResponse<M>,
{
    inner: Option<ResponseTypeItem<A, M>>,
}

/// Helper trait that converts compatible `ActorFuture` type to `Response`.
impl<A, M, T> std::convert::From<T> for Response<A, M>
    where A: Actor + MessageHandler<M>,
          T: ActorFuture<Item=A::Item, Error=A::Error, Actor=A> + Sized + 'static,
{
    fn from(fut: T) -> Response<A, M> {
        Response {inner: Some(ResponseTypeItem::Fut(Box::new(fut)))}
    }
}

impl<A, M> Response<A, M> where A: Actor + MessageResponse<M>
{
    /// Create response
    #[allow(non_snake_case)]
    pub fn Reply(val: A::Item) -> Self {
        Response {inner: Some(ResponseTypeItem::Item(val))}
    }

    /// Create async response
    #[allow(non_snake_case)]
    pub fn AsyncReply<T>(fut: T) -> Self
        where T: ActorFuture<Item=A::Item, Error=A::Error, Actor=A> + Sized + 'static
    {
        Response {inner: Some(ResponseTypeItem::Fut(Box::new(fut)))}
    }

    /// Create unit response
    #[allow(non_snake_case)]
    pub fn Empty() -> Self where A: MessageResponse<M, Item=()> {
        Response {inner: Some(ResponseTypeItem::Item(()))}
    }

    /// Create error response
    #[allow(non_snake_case)]
    pub fn Error(err: A::Error) -> Self {
        Response {inner: Some(ResponseTypeItem::Error(err))}
    }

    pub(crate) fn poll(&mut self, act: &mut A, ctx: &mut Context<A>) -> Poll<A::Item, A::Error>
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
