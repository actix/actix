use std;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{Canceled, Receiver, Sender};

use fut::ActorFuture;
use context::Context;
use address::MessageProxy;
use actor::{Actor, Message, MessageHandler};


#[must_use = "future do nothing unless polled"]
pub struct MessageResult<A, M>
    where A: Actor,
          M: Message,
{
    rx: Receiver<Result<M::Item, M::Error>>,
    act: PhantomData<A>,
}

impl<A, M> MessageResult<A, M> where A: Actor, M: Message
{
    pub(crate) fn new(rx: Receiver<Result<M::Item, M::Error>>) -> MessageResult<A, M> {
        MessageResult{rx: rx, act: PhantomData}
    }
}

impl<A, M> ActorFuture for MessageResult<A, M>
    where A: Actor,
          M: Message,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;
    type Actor = A;

    fn poll(&mut self, _: &mut A, _: &mut Context<A>) -> Poll<Self::Item, Self::Error>
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


enum MessageFutureItem<A, M>
    where A: Actor,
          M: Message,

{
    Item(M::Item),
    Error(M::Error),
    Fut(Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>)
}

pub struct MessageFuture<A, M> where A: Actor, M: Message,
{
    inner: Option<MessageFutureItem<A, M>>,
}

impl<A, M, T> std::convert::From<T> for MessageFuture<A, M>
    where A: Actor,
          M: Message,
          T: ActorFuture<Item=M::Item, Error=M::Error, Actor=A> + Sized + 'static,
{
    fn from(fut: T) -> MessageFuture<A, M> {
        MessageFuture {inner: Some(MessageFutureItem::Fut(Box::new(fut)))}
    }
}

pub trait MessageFutureResult<A, M>
    where A: Actor,
          M: Message<Item=Self>,
          Self: Sized + 'static
{
    fn to_result(self) -> MessageFuture<A, M>;
}

impl<A, M, T> MessageFutureResult<A, M> for T
    where A: Actor,
          M: Message<Item=Self>,
          Self: Sized + 'static
{
    fn to_result(self) -> MessageFuture<A, M> {
        MessageFuture {inner: Some(MessageFutureItem::Item(self))}
    }
}

pub trait MessageFutureError<A, M>
    where A: Actor,
          M: Message<Error=Self>,
          Self: Sized + 'static
{
    fn to_error(self) -> MessageFuture<A, M>;
}

impl<A, M, T> MessageFutureError<A, M> for T
    where A: Actor,
          M: Message<Error=Self>,
          Self: Sized + 'static
{
    fn to_error(self) -> MessageFuture<A, M> {
        MessageFuture {inner: Some(MessageFutureItem::Error(self))}
    }
}

impl<A, M> MessageFuture<A, M> where A: Actor, M: Message
{
    pub fn new<T>(fut: T) -> Self
        where T: ActorFuture<Item=M::Item, Error=M::Error, Actor=A> + Sized + 'static
    {
        MessageFuture {inner: Some(MessageFutureItem::Fut(Box::new(fut)))}
    }

    pub(crate) fn poll(&mut self, srv: &mut A, ctx: &mut Context<A>) -> Poll<M::Item, M::Error>
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
struct Envelope<A, M> where M: Message, A: Actor + MessageHandler<M> {
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<Sender<Result<M::Item, M::Error>>>,
}

impl<A, M> Envelope<A, M>
    where A: Actor + MessageHandler<M>,
          M: Message,
{
    pub(crate) fn new(msg: Option<M>,
                      tx: Option<Sender<Result<M::Item, M::Error>>>) -> Envelope<A, M>
    {
        Envelope{msg: msg, tx: tx, act: PhantomData}
    }
}


impl<A, M> MessageProxy for Envelope<A, M>
    where M: Message,
          A: Actor + MessageHandler<M>,
{
    type Actor = A;

    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut Context<A>)
    {
        if let Some(msg) = self.msg.take() {
            let fut = <Self::Actor as MessageHandler<M>>::handle(act, msg, ctx);
            let f: EnvelopFuture<Self::Actor, _> = EnvelopFuture {msg: PhantomData,
                                                                  fut: fut,
                                                                  tx: self.tx.take()};
            ctx.spawn(f);
        }
    }
}

pub(crate) struct EnvelopFuture<A, M> where M: Message, A: Actor
{
    msg: PhantomData<M>,
    fut: MessageFuture<A, M>,
    tx: Option<Sender<Result<M::Item, M::Error>>>,
}

impl<A, M> ActorFuture for EnvelopFuture<A, M>
    where M: Message, A: Actor
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut Context<A>) -> Poll<Self::Item, Self::Error>
    {
        match self.fut.poll(act, ctx) {
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
