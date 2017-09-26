use futures::Future;
use futures::unsync::mpsc::UnboundedSender;
use futures::unsync::oneshot::channel;

use context::Context;
use message::{Envelope, CallResult, MessageResult};
use actor::{Actor, MessageHandler};
pub use sync_address::SyncAddress;

#[doc(hidden)]
pub trait ActorAddress<A, T> where A: Actor {

    fn get(ctx: &mut Context<A>) -> T;
}

pub trait Subscriber<M: 'static> {

    /// Buffered send
    fn send(&self, msg: M);

    /// Unbuffered send
    fn unbuffered_send(&self, msg: M) -> Result<(), M>;
}

pub trait AsyncSubscriber<M> {

    type Future: Future;

    /// Send message, wait response asynchronously
    fn call(&self, msg: M) -> Self::Future;

    /// Send message, wait response asynchronously
    fn unbuffered_call(&self, msg: M) -> Result<Self::Future, M>;

}

pub(crate) trait MessageProxy {

    type Actor: Actor;

    /// handle message within new service and context
    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut Context<Self::Actor>);
}

pub(crate) struct BoxedMessageProxy<A>(pub(crate) Box<MessageProxy<Actor=A>>);

unsafe impl<T> Send for BoxedMessageProxy<T> {}

/// Address of the actor `A`
pub struct Address<A> where A: Actor {
    tx: UnboundedSender<BoxedMessageProxy<A>>
}

impl<A> Clone for Address<A> where A: Actor {
    fn clone(&self) -> Self {
        Address{tx: self.tx.clone() }
    }
}

impl<A> Address<A> where A: Actor {

    pub(crate) fn new(sender: UnboundedSender<BoxedMessageProxy<A>>) -> Address<A> {
        Address{tx: sender}
    }

    pub fn send<M: 'static>(&self, msg: M) where A: MessageHandler<M>
    {
        let _ = self.tx.unbounded_send(
            BoxedMessageProxy(Box::new(Envelope::new(Some(msg), None))));
    }

    pub fn call<B: Actor, M>(&self, msg: M) -> MessageResult<A, B, M>
        where A: MessageHandler<M>,
              M: 'static
    {
        let (tx, rx) = channel();
        let env = Envelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(BoxedMessageProxy(Box::new(env)));

        MessageResult::new(rx)
    }

    pub fn subscriber<M: 'static>(&self) -> Box<Subscriber<M>>
        where A: MessageHandler<M>
    {
        Box::new(self.clone())
    }
}

impl<A, M: 'static> Subscriber<M> for Address<A>
    where A: Actor + MessageHandler<M>
{

    fn send(&self, msg: M) {
        self.send(msg)
    }

    fn unbuffered_send(&self, msg: M) -> Result<(), M> {
        self.send(msg);
        Ok(())
    }
}

impl<A, M: 'static> AsyncSubscriber<M> for Address<A>
    where A: Actor + MessageHandler<M>
{
    type Future = CallResult<A::Item, A::Error>;

    fn call(&self, msg: M) -> Self::Future
    {
        let (tx, rx) = channel();
        let env = Envelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(BoxedMessageProxy(Box::new(env)));

        CallResult::new(rx)
    }

    fn unbuffered_call(&self, msg: M) -> Result<Self::Future, M>
    {
        let (tx, rx) = channel();
        let env = Envelope::new(Some(msg), Some(tx));
        let _ = self.tx.unbounded_send(BoxedMessageProxy(Box::new(env)));

        Ok(CallResult::new(rx))
    }
}

impl<A> ActorAddress<A, Address<A>> for A where A: Actor {

    fn get(ctx: &mut Context<A>) -> Address<A> {
        ctx.loc_address()
    }
}

impl<A> ActorAddress<A, ()> for A where A: Actor {

    fn get(_: &mut Context<A>) -> () {
        ()
    }
}
