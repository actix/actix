use actor::{Actor, AsyncContext};
use handler::{Handler, ResponseType};

use super::SendError;
use super::local_channel::{LocalSender, LocalAddrSender};
use super::local_message::{LocalRequest, LocalFutRequest, LocalSubscriberRequest};


/// Local address of the actor
///
/// Actor has to run in the same thread as owner of the address.
pub struct Address<A> where A: Actor, A::Context: AsyncContext<A> {
    tx: LocalAddrSender<A>
}

impl<A> Clone for Address<A> where A: Actor, A::Context: AsyncContext<A> {
    fn clone(&self) -> Self {
        Address{tx: self.tx.clone()}
    }
}

impl<A> Address<A> where A: Actor, A::Context: AsyncContext<A> {

    pub(crate) fn new(sender: LocalAddrSender<A>) -> Address<A> {
        Address{tx: sender}
    }

    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    /// Send message `M` to the actor `A`
    ///
    /// This method ignores receiver capacity, it silently fails if mailbox is closed.
    pub fn send<M>(&self, msg: M) where A: Handler<M>, M: ResponseType + 'static {
        let _ = self.tx.do_send(msg);
    }

    /// Try to send message `M` to the actor `A`
    ///
    /// This function fails if receiver if full or closed.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where A: Handler<M>, M: ResponseType + 'static
    {
        self.tx.try_send(msg, false)
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `LocalRequest` object get dropped, message cancels.
    pub fn call<B, M>(&self, _: &B, msg: M) -> LocalRequest<A, B, M>
        where A: Handler<M>, M: ResponseType + 'static, B: Actor, B::Context: AsyncContext<B>
    {
        match self.tx.send(msg) {
            Ok(rx) => LocalRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                LocalRequest::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                LocalRequest::new(None, None)
        }
    }

    /// Send message to the actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `LocalReceiver` object get dropped, message cancels.
    pub fn call_fut<M>(&self, msg: M) -> LocalFutRequest<A, M>
        where A: Handler<M>, M: ResponseType + 'static
    {
        match self.tx.send(msg) {
            Ok(rx) => LocalFutRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                LocalFutRequest::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                LocalFutRequest::new(None, None),
        }
    }

    /// Get `Subscriber` for specific message type
    pub fn into_subscriber<M>(self) -> Subscriber<M>
        where A: Handler<M>, M: ResponseType + 'static
    {
        Subscriber(self.tx.into_sender())
    }
}

/// `Subscriber` type allows to send one specific message to an actor.
///
/// You can get subscriber with `Address::into_subscriber::<M>()` method.
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub struct Subscriber<M: ResponseType+'static>(Box<LocalSender<M>>);

impl<M: ResponseType + 'static> Subscriber<M> {
    /// Send message
    ///
    /// Sends message even if actor's mailbox is full
    pub fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.0.do_send(msg)
    }

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
    pub fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.0.try_send(msg)
    }

    /// Send message and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Receiver`
    /// object get dropped, message cancels.
    pub fn call(&self, msg: M) -> LocalSubscriberRequest<M> {
        match self.0.send(msg) {
            Ok(rx) => LocalSubscriberRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                LocalSubscriberRequest::new(None, Some((self.0.boxed(), msg))),
            Err(SendError::Closed(_)) =>
                LocalSubscriberRequest::new(None, None),
        }
    }
}

impl<M: ResponseType + 'static> Clone for Subscriber<M> {
    fn clone(&self) -> Subscriber<M> {
        Subscriber(self.0.boxed())
    }
}
