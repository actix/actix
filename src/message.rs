use std::marker::PhantomData;

use futures::{Async, AsyncSink, Future, Poll};
use futures::sync::oneshot::{Receiver as SyncReceiver};
use futures::unsync::oneshot::{channel, Canceled, Sender, Receiver};

use address::Address;
use envelope::Envelope;
use queue::{unsync, MessageOption, Either};
use actor::{Actor, AsyncContext};
use fut::ActorFuture;
use handler::{Handler, ResponseType};
use context::ContextProtocol;

/// `Request` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct LocalRequest<A, B, M>
    where A: Actor + Handler<M>, M: ResponseType + 'static,
          B: Actor, B::Context: AsyncContext<B>
{
    rx: Option<Receiver<Result<M::Item, M::Error>>>,
    info: Option<(unsync::Sender<ContextProtocol<A>>, Sender<Result<M::Item, M::Error>>, M)>,
    act: PhantomData<B>,
}

impl<A, B, M> LocalRequest<A, B, M>
    where A: Actor + Handler<M>, M: ResponseType + 'static,
          B: Actor, B::Context: AsyncContext<B>
{
    pub(crate) fn new(rx: Option<Receiver<Result<M::Item, M::Error>>>,
                      info: Option<(unsync::Sender<ContextProtocol<A>>,
                                    Sender<Result<M::Item, M::Error>>, M)>) -> LocalRequest<A, B, M>
    {
        LocalRequest{rx: rx, info: info, act: PhantomData}
    }
}

impl<A, B, M> ActorFuture for LocalRequest<A, B, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
          B: Actor, B::Context: AsyncContext<B>,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;
    type Actor = B;

    fn poll(&mut self, _: &mut B, _: &mut B::Context) -> Poll<Self::Item, Self::Error> {
        // send message
        if let Some((sender, tx, msg)) = self.info.take() {
            let res = sender.send(move |opt| {
                match opt {
                    MessageOption::Message => Either::Message((msg, tx)),
                    MessageOption::Envelope => Either::Envelope(
                        ContextProtocol::Envelope(Envelope::local(msg, Some(tx), true)))
                }
            });

            match res {
                Ok(AsyncSink::Ready) => (),
                Ok(AsyncSink::NotReady(msg)) => {
                    self.info = Some((sender, msg.1, msg.0));
                    return Ok(Async::NotReady)
                }
                Err(_) => return Err(Canceled),
            }
        }

        if let Some(ref mut rx) = self.rx {
            rx.poll()
        } else {
            Err(Canceled)
        }
    }
}

/// `LocalFutRequest` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct LocalFutRequest<A, M>
    where A: Actor + Handler<M>, M: ResponseType + 'static,
{
    rx: Option<Receiver<Result<M::Item, M::Error>>>,
    info: Option<(unsync::Sender<ContextProtocol<A>>, Sender<Result<M::Item, M::Error>>, M)>,
}

impl<A, M> LocalFutRequest<A, M>
    where A: Actor + Handler<M>, M: ResponseType + 'static,
{
    pub(crate) fn new(rx: Option<Receiver<Result<M::Item, M::Error>>>,
                      info: Option<(unsync::Sender<ContextProtocol<A>>,
                                    Sender<Result<M::Item, M::Error>>, M)>)
                      -> LocalFutRequest<A, M>
    {
        LocalFutRequest{rx: rx, info: info}
    }
}

impl<A, M> Future for LocalFutRequest<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // send message
        if let Some((sender, tx, msg)) = self.info.take() {
            let res = sender.send(move |opt| {
                match opt {
                    MessageOption::Message => Either::Message((msg, tx)),
                    MessageOption::Envelope => Either::Envelope(
                        ContextProtocol::Envelope(Envelope::local(msg, Some(tx), true)))
                }
            });

            match res {
                Ok(AsyncSink::Ready) => (),
                Ok(AsyncSink::NotReady(msg)) => {
                    self.info = Some((sender, msg.1, msg.0));
                    return Ok(Async::NotReady)
                }
                Err(_) => return Err(Canceled),
            }
        }

        if let Some(ref mut rx) = self.rx {
            rx.poll()
        } else {
            Err(Canceled)
        }
    }
}

/// `Request` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct UpgradeAddress<A> where A: Actor {
    tx: Option<Sender<Address<A>>>,
    rx: Option<Receiver<Address<A>>>,
    proto: unsync::Sender<ContextProtocol<A>>,
}

impl<A> UpgradeAddress<A> where A: Actor
{
    pub(crate) fn new(proto: unsync::Sender<ContextProtocol<A>>) -> UpgradeAddress<A> {
        let (tx, rx) = channel();
        let res = proto.send(move |opt| match opt {
            MessageOption::Message => Either::Message(tx),
            MessageOption::Envelope => Either::Envelope(ContextProtocol::Upgrade(tx))
        });

        match res {
            Ok(AsyncSink::Ready) =>
                UpgradeAddress{tx: None, rx: Some(rx), proto: proto},
            Ok(AsyncSink::NotReady(tx)) =>
                UpgradeAddress{tx: Some(tx), rx: Some(rx), proto: proto},
            Err(_) =>
                UpgradeAddress{tx: None, rx: None, proto: proto},
        }
    }
}

impl<A> Future for UpgradeAddress<A>
    where A: Actor, A::Context: AsyncContext<A>
{
    type Item = Address<A>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // send message
        if let Some(tx) = self.tx.take() {
            let res = self.proto.send(move |opt| match opt {
                MessageOption::Message => Either::Message(tx),
                MessageOption::Envelope => Either::Envelope(ContextProtocol::Upgrade(tx))
            });

            match res {
                Ok(AsyncSink::Ready) => (),
                Ok(AsyncSink::NotReady(msg)) => {
                    self.tx = Some(msg);
                    return Ok(Async::NotReady)
                }
                Err(_) => return Err(Canceled),
            }
        }

        if let Some(ref mut rx) = self.rx {
            rx.poll()
        } else {
            Err(Canceled)
        }
    }
}

/// `Request` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<A, B, M>
    where A: Actor + Handler<M>, M: ResponseType,
          B: Actor, B::Context: AsyncContext<B>
{
    rx: SyncReceiver<Result<M::Item, M::Error>>,
    act_a: PhantomData<A>,
    act_b: PhantomData<B>,
}

impl<A, B, M> Request<A, B, M>
    where A: Actor + Handler<M>, M: ResponseType,
          B: Actor, B::Context: AsyncContext<B>
{
    pub(crate) fn new(rx: SyncReceiver<Result<M::Item, M::Error>>) -> Request<A, B, M> {
        Request{rx: rx, act_a: PhantomData, act_b: PhantomData}
    }
}

impl<A, B, M> ActorFuture for Request<A, B, M>
    where A: Actor + Handler<M>, M: ResponseType,
          B: Actor, B::Context: AsyncContext<B>,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;
    type Actor = B;

    fn poll(&mut self, _: &mut B, _: &mut B::Context) -> Poll<Self::Item, Self::Error> {
        self.rx.poll()
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
