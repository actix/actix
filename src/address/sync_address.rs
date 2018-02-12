use handler::ResponseType;

use super::{SendError, SyncSubscriberRequest};
use super::sync_channel::SyncSender;

/*
/// `Send` address of the actor. Actor can run in different thread
pub struct SyncAddress<A> where A: Actor {
    tx: AddressSender<A>
}

unsafe impl<A> Send for SyncAddress<A> where A: Actor {}
unsafe impl<A> Sync for SyncAddress<A> where A: Actor {}

impl<A> Clone for SyncAddress<A> where A: Actor {
    fn clone(&self) -> Self {
        SyncAddress{tx: self.tx.clone()}
    }
}

impl<A> SyncAddress<A> where A: Actor {

    pub(crate) fn new(sender: AddressSender<A>) -> SyncAddress<A> {
        SyncAddress{tx: sender}
    }

    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    /// Send message `M` to actor `A`
    ///
    /// This function ignores receiver capacity and always successed.
    /// Message cold be sent to an actor running in different thread.
    pub fn send<M>(&self, msg: M)
        where A: Handler<M>, <A as Actor>::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
    {
        let _ = self.tx.do_send(msg);
    }

    /// Send message `M` to actor `A`
    ///
    /// This function fails if receiver if full or closed.
    /// Message cold be sent to actor running in different thread.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where A: Handler<M>, <A as Actor>::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
    {
        self.tx.try_send(msg, false)
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// if returned `Request` object get dropped, message cancels.
    pub fn call<B: Actor, M>(&self, _: &B, msg: M) -> Request<A, B, M>
        where A: Handler<M>, A::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
    {
        match self.tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                Request::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                Request::new(None, None),
        }
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// if returned `Receiver` object get dropped, message cancels.
    pub fn call_fut<M>(&self, msg: M) -> RequestFut<A, M>
        where A: Handler<M>, A::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
    {
        match self.tx.send(msg) {
            Ok(rx) => RequestFut::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                RequestFut::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                RequestFut::new(None, None),
        }
    }

    /// Convert address to a `Subscriber` for specific message type
    pub fn subscriber<M>(self) -> SyncSubscriber<M>
        where A: Handler<M>, A::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send
    {
        SyncSubscriber{tx: self.tx.into_sender()}
    }
}

impl<A, M> From<SyncAddress<A>> for SyncSubscriber<M>
    where A: Actor + Handler<M>, A::Context: ToEnvelope<A>,
          M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send
{
    fn from(addr: SyncAddress<A>) -> SyncSubscriber<M> {
        SyncSubscriber{tx: addr.tx.into_sender()}
    }
}*/

/// `SyncSubscriber` type allows to send one specific message to an actor.
///
/// You can get subscriber with `SyncAddress::into_subscriber::<M>()` method.
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub struct SyncSubscriber<M>
    where M: ResponseType + Send + 'static,
          M::Item: Send, M::Error: Send,
{
    pub(crate) tx: Box<SyncSender<M>>
}

unsafe impl<M> Send for SyncSubscriber<M>
    where M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send {}

impl<M> SyncSubscriber<M>
    where M: ResponseType + Send + 'static,
          M::Item: Send, M::Error: Send
{

    /// Send message
    ///
    /// Sends message even if actor's mailbox is full
    pub fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.do_send(msg)
    }

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
    pub fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.try_send(msg)
    }

    /// Send message and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Receiver`
    /// object get dropped, message cancels.
    pub fn call(&self, msg: M) -> SyncSubscriberRequest<M> {
        match self.tx.send(msg) {
            Ok(rx) => SyncSubscriberRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                SyncSubscriberRequest::new(None, Some((self.tx.boxed(), msg))),
            Err(SendError::Closed(_)) =>
                SyncSubscriberRequest::new(None, None),
        }
    }
}

impl<M> Clone for SyncSubscriber<M>
    where M: ResponseType + Send + 'static,
          M::Item: Send, M::Error: Send
{
    fn clone(&self) -> SyncSubscriber<M> {
        SyncSubscriber{tx: self.tx.boxed()}
    }
}
