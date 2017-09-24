use futures::unsync::mpsc::UnboundedSender;
use futures::unsync::oneshot::channel;

use context::Context;
use message::{Envelope, CallResult, MessageResult};
use service::{Message, MessageHandler, Service};


pub trait Subscriber<M> {
    /// Buffered send
    fn send(&self, msg: M);

    /// Send message, wait response asynchronously
    fn call(&self, msg: M) -> CallResult<M> where M: Message;

    /// Unbuffered send
    fn unbuffered_send(&self, msg: M) -> Result<(), M>;

    /// Send message, wait response asynchronously
    fn unbuffered_call(&self, msg: M) -> Result<CallResult<M>, M> where M: Message;

}

pub(crate) type BoxedMessageProxy<T> = Box<MessageProxy<Service=T>>;

pub struct Address<T> where T: Service {
    tx: UnboundedSender<BoxedMessageProxy<T>>
}

impl<T> Clone for Address<T> where T: Service {
    fn clone(&self) -> Self {
        Address{tx: self.tx.clone() }
    }
}

impl<T> Address<T> where T: Service {

    pub(crate) fn new(sender: UnboundedSender<BoxedMessageProxy<T>>) -> Address<T> {
        Address{tx: sender}
    }

    pub fn send<M: Message>(&self, msg: M) where T: MessageHandler<M>
    {
        let _ = self.tx.unbounded_send(
            Box::new(Envelope::new(Some(msg), None)));
    }

    pub fn call<M: Message, S: Service>(&self, msg: M) -> MessageResult<M, S>
        where T: MessageHandler<M>
    {
        let (tx, rx) = channel();
        let env = Envelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(Box::new(env));

        MessageResult::new(rx)
    }

    pub fn subscriber<M: Message>(&self) -> Box<Subscriber<M>>
        where T: MessageHandler<M>
    {
        Box::new(self.clone())
    }
}

impl<T, M> Subscriber<M> for Address<T>
    where M: Message,
          T: Service + MessageHandler<M>
{
    fn send(&self, msg: M) {
        self.send(msg)
    }

    fn unbuffered_send(&self, msg: M) -> Result<(), M> {
        self.send(msg);
        Ok(())
    }

    fn call(&self, msg: M) -> CallResult<M>
    {
        let (tx, rx) = channel();
        let env = Envelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(Box::new(env));

        CallResult::new(rx)
    }

    fn unbuffered_call(&self, msg: M) -> Result<CallResult<M>, M>
    {
        let (tx, rx) = channel();
        let env = Envelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(Box::new(env));

        Ok(CallResult::new(rx))
    }
}

pub(crate) trait MessageProxy {

    type Service: Service;

    /// handle message within new service and context
    fn handle(&mut self, srv: &mut Self::Service, ctx: &mut Context<Self::Service>);
}
