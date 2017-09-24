use futures::unsync::mpsc::UnboundedSender;
use futures::unsync::oneshot::channel;

use context::Context;
use message::{Msg, MessageResult};
use service::{Message, MessageHandler, Service};


pub trait Subscriber<M> where M: Message {
    fn tell(&self, msg: M);
    // fn send(&self, msg: M) -> MessageResult<M>;
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

    pub fn tell<M: Message>(&self, msg: M) where T: MessageHandler<M>
    {
        let _ = self.tx.unbounded_send(
            Box::new(Msg::new(Some(msg), None)));
    }

    pub fn send<M: Message, S: Service>(&self, msg: M) -> MessageResult<M, S>
        where T: MessageHandler<M>
    {
        let (tx, rx) = channel();
        let msg = Msg::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(Box::new(msg));

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
    fn tell(&self, msg: M) {
        self.tell(msg)
    }

    //fn send(&self, msg: M) -> MessageResult<M> {
    //    msg.send(&self)
    //}
}


pub(crate) trait MessageProxy {

    type Service: Service;

    /// handle message within new service and context
    fn handle(&mut self, srv: &mut Self::Service, ctx: &mut Context<Self::Service>);
}
