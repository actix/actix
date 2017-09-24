use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{Canceled, Receiver, Sender};

use fut;
use context::Context;
use address::MessageProxy;
use service::{Message, MessageFuture, MessageHandler, Service};


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

pub(crate)
struct Msg<T, S> where T: Message, S: Service + MessageHandler<T>, {
    msg: Option<T>,
    srv: PhantomData<S>,
    tx: Option<Sender<Result<T::Item, T::Error>>>,
}

impl<T, S> Msg<T, S>
    where T: Message,
          S: Service + MessageHandler<T>,
{
    pub(crate) fn new(msg: Option<T>,
                      tx: Option<Sender<Result<T::Item, T::Error>>>) -> Msg<T, S>
    {
        Msg{msg: msg, tx: tx, srv: PhantomData}
    }
}


impl<T, S> MessageProxy for Msg<T, S>
    where T: Message,
          S: Service + MessageHandler<T>,
{
    type Service = S;

    fn handle(&mut self, srv: &mut Self::Service, ctx: &mut Context<S>)
    {
        if let Some(msg) = self.msg.take() {
            let fut = <Self::Service as MessageHandler<T>>::handle(srv, msg, ctx);
            let f: MsgFuture<T, Self::Service> = MsgFuture {msg: PhantomData,
                                                            fut: fut,
                                                            tx: self.tx.take()};
            ctx.spawn(f);
        }
    }
}

pub(crate)
struct MsgFuture<T, S>
    where T: Message, S: Service
{
    msg: PhantomData<T>,
    fut: MessageFuture<T, S>,
    tx: Option<Sender<Result<T::Item, T::Error>>>,
}

impl<T, S> fut::CtxFuture for MsgFuture<T, S>
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
