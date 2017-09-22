use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{channel, Canceled, Receiver, Sender};

use fut;
use context::Context;
use address::{Address, MessageProxy};
use service::{Message, MessageFuture, Service};


pub trait MessageTransport {

    type Message: Message;

    /// Send message and return the result asynchronously.
    fn send_to(self, dest: &Address<<Self::Message as Message>::Service>)
               -> MessageResult<Self::Message>;
}

pub struct MessageResult<T> where T: Message {
    rx: Receiver<Result<T::Item, T::Error>>,
}

impl<T> Future for MessageResult<T> where T: Message
{
    type Item = Result<T::Item, T::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
    {
        match self.rx.poll() {
            Ok(Async::Ready(val)) =>
                Ok(Async::Ready(val)),
            Ok(Async::NotReady) =>
                Ok(Async::NotReady),
            Err(err) => {
                Err(err)
            }
        }
    }
}

pub(crate)
struct Msg<T> where T: Message {
    msg: T,
    tx: Option<Sender<Result<T::Item, T::Error>>>,
}

impl<T, S> MessageProxy for Msg<T>
    where T: Message<Service=S>,
          S: Service<Context=Context<<T as Message>::Service>>,
{
    type Service = T::Service;

    fn handle(&mut self, srv: &mut T::Service, ctx: &mut <T::Service as Service>::Context)
    {
        let fut = self.msg.handle(srv, ctx);
        let f: MsgFuture<T> = MsgFuture {msg: PhantomData,
                                         fut: fut,
                                         tx: Some(self.tx.take().unwrap())};
        ctx.spawn(f);
    }
}

impl<T> MessageTransport for T
    where T: Message,
          T::Service: Service<Context=Context<<T as Message>::Service>>,
{
    type Message = T;

    fn send_to(self, dest: &Address<<Self::Message as Message>::Service>)
               -> MessageResult<Self::Message>
    {
        let (tx, rx) = channel();
        let msg = Msg{msg: self, tx: Some(tx)};
        dest.send(msg);

        MessageResult{rx: rx}
    }
}

pub(crate)
struct MsgFuture<T>
    where T: Message,
          T::Service: Service<Context=Context<<T as Message>::Service>>,
{
    msg: PhantomData<T>,
    fut: MessageFuture<T>,
    tx: Option<Sender<Result<T::Item, T::Error>>>,
}

impl<T> fut::CtxFuture for MsgFuture<T>
    where T: Message,
          T::Service: Service<Context=Context<<T as Message>::Service>>
{
    type Item = ();
    type Error = ();
    type Service = T::Service;

    fn poll(&mut self, srv: &mut Self::Service, ctx: &mut <Self::Service as Service>::Context)
            -> Poll<Self::Item, Self::Error>
    {
        match self.fut.poll(srv, ctx) {
            Ok(Async::Ready(val)) => {
                let _ = self.tx.take().expect("Multiple polls?").send(Ok(val));
                Ok(Async::Ready(()))
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                let _ = self.tx.take().expect("Multiple polls?").send(Err(err));
                Err(())
            }
        }
    }
}
