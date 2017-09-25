use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::sync::mpsc::UnboundedSender;
use futures::sync::oneshot::{channel, Canceled, Receiver, Sender};

use fut::ActorFuture;
use actor::{Actor, Message, MessageHandler};
use address::{Subscriber, AsyncSubscriber, MessageProxy, BoxedMessageProxy};
use context::Context;
use message::MessageFuture;


pub struct SyncAddress<A> where A: Actor {
    tx: UnboundedSender<BoxedMessageProxy<A>>
}

impl<A> Clone for SyncAddress<A> where A: Actor {
    fn clone(&self) -> Self {
        SyncAddress{tx: self.tx.clone()}
    }
}

impl<A> SyncAddress<A> where A: Actor {

    pub(crate) fn new(sender: UnboundedSender<BoxedMessageProxy<A>>) -> SyncAddress<A> {
        SyncAddress{tx: sender}
    }

    pub fn send<M: Message + Send>(&self, msg: M)
        where A: MessageHandler<M>,
              M::Item: Send,
              M::Error: Send,
    {
        let _ = self.tx.unbounded_send(
            BoxedMessageProxy(Box::new(SyncEnvelope::new(Some(msg), None))));
    }

    pub fn call<B: Actor, M: Message + Send>(&self, msg: M) -> MessageResult<B, M>
        where A: MessageHandler<M>,
              M::Item: Send,
              M::Error: Send,
    {
        let (tx, rx) = channel();
        let env = SyncEnvelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(BoxedMessageProxy(Box::new(env)));

        MessageResult::new(rx)
    }

    pub fn subscriber<M: Message + Send>(&self) -> Box<Subscriber<M>>
        where A: MessageHandler<M>,
              M::Item: Send,
              M::Error: Send,
    {
        Box::new(self.clone())
    }

    pub fn async_subscriber<M>(&self) -> Box<AsyncSubscriber<M, Future=CallResult<M>>>
        where A: MessageHandler<M>,
              M::Item: Send,
              M::Error: Send,
              M: Message + Send,
    {
        Box::new(self.clone())
    }
}

impl<A, M> Subscriber<M> for SyncAddress<A>
    where M: Message + Send,
          M::Item: Send,
          M::Error: Send,
          A: Actor + MessageHandler<M>
{
    fn send(&self, msg: M) {
        self.send(msg)
    }

    fn unbuffered_send(&self, msg: M) -> Result<(), M> {
        self.send(msg);
        Ok(())
    }
}

impl<A, M> AsyncSubscriber<M> for SyncAddress<A>
    where M: Message + Send,
          M::Item: Send,
          M::Error: Send,
          A: Actor + MessageHandler<M>
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

struct SyncEnvelope<A, M> where M: Message, A: Actor + MessageHandler<M>
{
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<Sender<Result<M::Item, M::Error>>>,
}

impl<A, M> SyncEnvelope<A, M> where M: Message, A: Actor + MessageHandler<M>
{
    fn new(msg: Option<M>,
           tx: Option<Sender<Result<M::Item, M::Error>>>) -> SyncEnvelope<A, M>
    {
        SyncEnvelope{msg: msg, tx: tx, act: PhantomData}
    }
}

impl<A, M> MessageProxy for SyncEnvelope<A, M>
    where M: Message, A: Actor + MessageHandler<M>,
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

struct EnvelopFuture<A, M> where M: Message, A: Actor
{
    msg: PhantomData<M>,
    fut: MessageFuture<A, M>,
    tx: Option<Sender<Result<M::Item, M::Error>>>,
}

impl<A, M> ActorFuture for EnvelopFuture<A, M> where M: Message, A: Actor
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

#[must_use = "future do nothing unless polled"]
pub struct MessageResult<A, M>
    where M: Message,
          A: Actor
{
    rx: Receiver<Result<M::Item, M::Error>>,
    act: PhantomData<A>,
}

impl<A, M> MessageResult<A, M> where M: Message, A: Actor
{
    pub(crate) fn new(rx: Receiver<Result<M::Item, M::Error>>) -> MessageResult<A, M> {
        MessageResult{rx: rx, act: PhantomData}
    }
}

impl<A, M> ActorFuture for MessageResult<A, M>
    where M: Message,
          A: Actor
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
