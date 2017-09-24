use futures::unsync::mpsc::UnboundedSender;

use context::Context;
use service::Service;


pub(crate) type BoxedMessageProxy<T> = Box<MessageProxy<Service=T>>;

pub struct Address<T> where T: Service<Context=Context<T>> {
    tx: UnboundedSender<BoxedMessageProxy<T>>
}

impl<T> Clone for Address<T> where T: Service<Context=Context<T>> {
    fn clone(&self) -> Self {
        Address{tx: self.tx.clone() }
    }
}

impl<T> Address<T> where T: Service<Context=Context<T>> {

    pub(crate) fn new(sender: UnboundedSender<BoxedMessageProxy<T>>) -> Address<T> {
        Address{tx: sender}
    }

    pub(crate) fn send(&self, msg: Box<MessageProxy<Service=T>>)
        where T: Service<Context=Context<T>>,
              //M: MessageProxy<Service=T> + 'static
    {
        let _ = self.tx.unbounded_send(msg);
    }
}

pub(crate) trait MessageProxy {

    type Service: Service<Context=Context<Self::Service>>;

    /// handle message within new service and context
    fn handle(&mut self,
              srv: &mut Self::Service,
              ctx: &mut <Self::Service as Service>::Context);
}
