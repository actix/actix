use std;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{Canceled, Receiver, Sender};

use fut::{self, CtxFuture};
use context::Context;
use address::MessageProxy;
use service::{Message, MessageHandler, Service};


#[must_use = "future do nothing unless polled"]
pub struct MessageResult<T, S>
    where T: Message,
          S: Service
{
    rx: Receiver<Result<T::Item, T::Error>>,
    srv: PhantomData<S>,
}

impl<T, S> MessageResult<T, S> where T: Message, S: Service
{
    pub(crate) fn new(rx: Receiver<Result<T::Item, T::Error>>) -> MessageResult<T, S> {
        MessageResult{rx: rx, srv: PhantomData}
    }
}

impl<T, S> fut::CtxFuture for MessageResult<T, S>
    where T: Message,
          S: Service
{
    type Item = Result<T::Item, T::Error>;
    type Error = Canceled;
    type Service = S;

    fn poll(&mut self, _: &mut S, _: &mut Context<S>) -> Poll<Self::Item, Self::Error>
    {
        self.rx.poll()
    }
}

#[must_use = "future do nothing unless polled"]
pub struct CallResult<M> where M: Message
{
    rx: Receiver<Result<M::Item, M::Error>>,
}

impl<M> CallResult<M> where M: Message
{
    pub(crate) fn new(rx: Receiver<Result<M::Item, M::Error>>) -> CallResult<M> {
        CallResult{rx: rx}
    }
}

impl<M> Future for CallResult<M> where M: Message
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
    {
        self.rx.poll()
    }
}


enum MessageFutureItem<M, S>
    where M: Message,
          S: Service,
{
    Item(M::Item),
    Error(M::Error),
    Fut(Box<CtxFuture<Item=M::Item, Error=M::Error, Service=S>>)
}

pub struct MessageFuture<M, S>
    where M: Message,
          S: Service,
{
    inner: Option<MessageFutureItem<M, S>>,
}

impl<T, M, S> std::convert::From<T> for MessageFuture<M, S>
    where M: Message,
          S: Service,
          T: CtxFuture<Item=M::Item, Error=M::Error, Service=S> + Sized + 'static,
{
    fn from(fut: T) -> MessageFuture<M, S> {
        MessageFuture {inner: Some(MessageFutureItem::Fut(Box::new(fut)))}
    }
}

pub trait MessageFutureResult<M, S>
    where M: Message<Item=Self>,
          S: Service,
          Self: Sized + 'static
{
    fn to_result(self) -> MessageFuture<M, S>;
}

impl<T, M, S> MessageFutureResult<M, S> for T
    where M: Message<Item=Self>,
          S: Service,
          Self: Sized + 'static
{
    fn to_result(self) -> MessageFuture<M, S> {
        MessageFuture {inner: Some(MessageFutureItem::Item(self))}
    }
}

pub trait MessageFutureError<M, S>
    where M: Message<Error=Self>,
          S: Service,
          Self: Sized + 'static
{
    fn to_error(self) -> MessageFuture<M, S>;
}

impl<T, M, S> MessageFutureError<M, S> for T
    where M: Message<Error=Self>,
          S: Service,
          Self: Sized + 'static
{
    fn to_error(self) -> MessageFuture<M, S> {
        MessageFuture {inner: Some(MessageFutureItem::Error(self))}
    }
}

impl<M, S> MessageFuture<M, S> where M: Message, S: Service
{
    pub fn new<T>(fut: T) -> Self
        where T: CtxFuture<Item=M::Item, Error=M::Error, Service=S> + Sized + 'static
    {
        MessageFuture {inner: Some(MessageFutureItem::Fut(Box::new(fut)))}
    }

    pub(crate) fn poll(&mut self, srv: &mut S, ctx: &mut Context<S>) -> Poll<M::Item, M::Error>
    {
        if let Some(item) = self.inner.take() {
            match item {
                MessageFutureItem::Fut(mut fut) => {
                    match fut.poll(srv, ctx) {
                        Ok(Async::NotReady) => {
                            self.inner = Some(MessageFutureItem::Fut(fut));
                            return Ok(Async::NotReady)
                        }
                        result => return result
                    }
                }
                MessageFutureItem::Item(item) => return Ok(Async::Ready(item)),
                MessageFutureItem::Error(err) => return Err(err)
            }
        }
        Ok(Async::NotReady)
    }
}

pub(crate)
struct Envelope<T, S> where T: Message, S: Service + MessageHandler<T> {
    msg: Option<T>,
    srv: PhantomData<S>,
    tx: Option<Sender<Result<T::Item, T::Error>>>,
}

impl<T, S> Envelope<T, S>
    where T: Message,
          S: Service + MessageHandler<T>,
{
    pub(crate) fn new(msg: Option<T>,
                      tx: Option<Sender<Result<T::Item, T::Error>>>) -> Envelope<T, S>
    {
        Envelope{msg: msg, tx: tx, srv: PhantomData}
    }
}


impl<T, S> MessageProxy for Envelope<T, S>
    where T: Message,
          S: Service + MessageHandler<T>,
{
    type Service = S;

    fn handle(&mut self, srv: &mut Self::Service, ctx: &mut Context<S>)
    {
        if let Some(msg) = self.msg.take() {
            let fut = <Self::Service as MessageHandler<T>>::handle(srv, msg, ctx);
            let f: EnvelopFuture<_, Self::Service> = EnvelopFuture {msg: PhantomData,
                                                                    fut: fut,
                                                                    tx: self.tx.take()};
            ctx.spawn(f);
        }
    }
}

pub(crate) struct EnvelopFuture<T, S> where T: Message, S: Service
{
    msg: PhantomData<T>,
    fut: MessageFuture<T, S>,
    tx: Option<Sender<Result<T::Item, T::Error>>>,
}

impl<T, S> fut::CtxFuture for EnvelopFuture<T, S>
    where T: Message, S: Service
{
    type Item = ();
    type Error = ();
    type Service = S;

    fn poll(&mut self, srv: &mut S, ctx: &mut Context<S>) -> Poll<Self::Item, Self::Error>
    {
        match self.fut.poll(srv, ctx) {
            Ok(Async::Ready(val)) => {
                if let Some(tx) = self.tx.take() {
                    let _ = tx.send(Ok(val));
                }
                Ok(Async::Ready(()))
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                if let Some(tx) = self.tx.take() {
                    let _ = tx.send(Err(err));
                }
                Err(())
            }
        }
    }
}
