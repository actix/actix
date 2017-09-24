use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{channel, Canceled, Receiver, Sender};

use fut;
use context::Context;
use address::{Address, MessageProxy};
use service::{Message, MessageFuture, MessageHandler, Service};


pub trait MessageTransport {

    type Message: Message;

    /// Send message, do not wait for result
    fn tell<S>(self, dest: &Address<S>)
        where S: Service<Context=Context<S>> + MessageHandler<Self::Message>;

    /// Send message and return the result asynchronously.
    fn send<S>(self, dest: &Address<S>) -> MessageResult<Self::Message>
        where S: Service<Context=Context<S>> + MessageHandler<Self::Message>;
}

#[must_use = "future do nothing unless polled"]
pub struct MessageResult<T> where T: Message {
    rx: Receiver<Result<T::Item, T::Error>>,
}

impl<T> Future for MessageResult<T> where T: Message
{
    type Item = Result<T::Item, T::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
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

impl<T, S> MessageProxy for Msg<T, S>
    where T: Message,
          S: Service<Context=Context<S>> + MessageHandler<T>,
{
    type Service = S;

    fn handle(&mut self,
              srv: &mut Self::Service,
              ctx: &mut <Self::Service as Service>::Context)
    {
        //let fut = self.msg.handle(srv, ctx);
        if let Some(msg) = self.msg.take() {
            let fut = <Self::Service as MessageHandler<T>>::handle(srv, msg, ctx);
            let f: MsgFuture<T, Self::Service> = MsgFuture {msg: PhantomData,
                                                            fut: fut,
                                                            tx: self.tx.take()};
            ctx.spawn(f);
        }
    }
}

impl<T> MessageTransport for T
    where T: Message
{
    type Message = T;

    fn tell<S: Service<Context=Context<S>> + MessageHandler<T>>(self, dest: &Address<S>) {
        dest.send(Box::new(Msg{msg: Some(self), tx: None, srv: PhantomData}));
    }

    fn send<S: Service<Context=Context<S>> + MessageHandler<T>>(self, dest: &Address<S>)
                                            -> MessageResult<Self::Message>
    {
        let (tx, rx) = channel();
        let msg = Msg{msg: Some(self), tx: Some(tx), srv: PhantomData};
        dest.send(Box::new(msg));

        MessageResult{rx: rx}
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

    fn poll(&mut self, srv: &mut S, ctx: &mut S::Context) -> Poll<Self::Item, Self::Error>
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
