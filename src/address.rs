use futures::unsync::mpsc::UnboundedSender;

use service::Service;
use context::Context;


pub(crate) type BoxedMessageProxy<T> = Box<MessageProxy<Service=T>>;

pub struct Address<T>(UnboundedSender<BoxedMessageProxy<T>>);

impl<T> Clone for Address<T> {
    fn clone(&self) -> Self {
        Address(self.0.clone())
    }
}

impl<T> Address<T> where T: Service {

    pub(crate) fn new(sender: UnboundedSender<BoxedMessageProxy<T>>) -> Address<T> {
        Address(sender)
    }

    pub(crate) fn send<M>(&self, msg: M)
        where T: Service<Context=Context<T>>, M: MessageProxy<Service=T> + 'static
    {
        let _ = self.0.unbounded_send(Box::new(msg));
    }
}

pub(crate) trait MessageProxy {

    type Service: Service<Context=Context<Self::Service>>;

    /// handle message within new service and context
    fn handle(&mut self,
              srv: &mut Self::Service,
              ctx: &mut <Self::Service as Service>::Context);
}
