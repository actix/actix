use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::sync::mpsc::UnboundedSender;
use futures::sync::oneshot::{channel, Canceled, Receiver, Sender};

use fut;
use address::{Subscriber, AsyncSubscriber, MessageProxy, BoxedMessageProxy};
use context::Context;
use message::MessageFuture;
use service::{Message, MessageHandler, Service};


pub struct SyncAddress<T> where T: Service {
    tx: UnboundedSender<BoxedMessageProxy<T>>
}

impl<T> Clone for SyncAddress<T> where T: Service {
    fn clone(&self) -> Self {
        SyncAddress{tx: self.tx.clone()}
    }
}

impl<T> SyncAddress<T> where T: Service {

    pub(crate) fn new(sender: UnboundedSender<BoxedMessageProxy<T>>) -> SyncAddress<T> {
        SyncAddress{tx: sender}
    }

    pub fn send<M: Message + Send>(&self, msg: M)
        where T: MessageHandler<M>,
              M::Item: Send,
              M::Error: Send,
    {
        let _ = self.tx.unbounded_send(
            BoxedMessageProxy(Box::new(SyncEnvelope::new(Some(msg), None))));
    }

    pub fn call<M: Message + Send, S: Service>(&self, msg: M) -> MessageResult<M, S>
        where T: MessageHandler<M>,
              M::Item: Send,
              M::Error: Send,
    {
        let (tx, rx) = channel();
        let env = SyncEnvelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(BoxedMessageProxy(Box::new(env)));

        MessageResult::new(rx)
    }

    pub fn subscriber<M: Message + Send>(&self) -> Box<Subscriber<M>>
        where T: MessageHandler<M>,
              M::Item: Send,
              M::Error: Send,
    {
        Box::new(self.clone())
    }

    pub fn async_subscriber<M>(&self) -> Box<AsyncSubscriber<M, Future=CallResult<M>>>
        where T: MessageHandler<M>,
              M::Item: Send,
              M::Error: Send,
              M: Message + Send,
    {
        Box::new(self.clone())
    }
}

impl<T, M> Subscriber<M> for SyncAddress<T>
    where M: Message + Send,
          M::Item: Send,
          M::Error: Send,
          T: Service + MessageHandler<M>
{
    fn send(&self, msg: M) {
        self.send(msg)
    }

    fn unbuffered_send(&self, msg: M) -> Result<(), M> {
        self.send(msg);
        Ok(())
    }
}

impl<T, M> AsyncSubscriber<M> for SyncAddress<T>
    where M: Message + Send,
          M::Item: Send,
          M::Error: Send,
          T: Service + MessageHandler<M>
{
    type Future = CallResult<M>;

    fn call(&self, msg: M) -> CallResult<M>
    {
        let (tx, rx) = channel();
        let env = SyncEnvelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(BoxedMessageProxy(Box::new(env)));

        CallResult::new(rx)
    }

    fn unbuffered_call(&self, msg: M) -> Result<CallResult<M>, M>
    {
        Ok(AsyncSubscriber::call(self, msg))
    }
}

struct SyncEnvelope<M, S> where M: Message, S: Service + MessageHandler<M>
{
    msg: Option<M>,
    srv: PhantomData<S>,
    tx: Option<Sender<Result<M::Item, M::Error>>>,
}

impl<M, S> SyncEnvelope<M, S> where M: Message, S: Service + MessageHandler<M>
{
    fn new(msg: Option<M>,
           tx: Option<Sender<Result<M::Item, M::Error>>>) -> SyncEnvelope<M, S>
    {
        SyncEnvelope{msg: msg, tx: tx, srv: PhantomData}
    }
}

impl<M, S> MessageProxy for SyncEnvelope<M, S>
    where M: Message, S: Service + MessageHandler<M>,
{
    type Service = S;

    fn handle(&mut self, srv: &mut Self::Service, ctx: &mut Context<S>)
    {
        if let Some(msg) = self.msg.take() {
            let fut = <Self::Service as MessageHandler<M>>::handle(srv, msg, ctx);
            let f: EnvelopFuture<_, Self::Service> = EnvelopFuture {msg: PhantomData,
                                                                    fut: fut,
                                                                    tx: self.tx.take()};
            ctx.spawn(f);
        }
    }
}

struct EnvelopFuture<M, S> where M: Message, S: Service
{
    msg: PhantomData<M>,
    fut: MessageFuture<M, S>,
    tx: Option<Sender<Result<M::Item, M::Error>>>,
}

impl<M, S> fut::CtxFuture for EnvelopFuture<M, S> where M: Message, S: Service
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

#[must_use = "future do nothing unless polled"]
pub struct MessageResult<M, S>
    where M: Message,
          S: Service
{
    rx: Receiver<Result<M::Item, M::Error>>,
    srv: PhantomData<S>,
}

impl<M, S> MessageResult<M, S> where M: Message, S: Service
{
    pub(crate) fn new(rx: Receiver<Result<M::Item, M::Error>>) -> MessageResult<M, S> {
        MessageResult{rx: rx, srv: PhantomData}
    }
}

impl<M, S> fut::CtxFuture for MessageResult<M, S>
    where M: Message,
          S: Service
{
    type Item = Result<M::Item, M::Error>;
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
    fn new(rx: Receiver<Result<M::Item, M::Error>>) -> CallResult<M> {
        CallResult{rx: rx}
    }
}

impl<M> Future for CallResult<M> where M: Message
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.rx.poll()
    }
}
