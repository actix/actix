use futures::sync::oneshot::{Sender, Receiver};

use actor::Actor;
use handler::{Handler, Message};

use super::envelope::{ToEnvelope, SyncEnvelope};
use super::sync_channel::{SyncSender, SyncAddressSender};
use super::{Request, SyncSubscriberRequest};
use super::{Destination, MessageDestination, SendError};


/// Sync destination of the actor. Actor can run in different thread
pub struct Syn;

impl<A: Actor> Destination<A> for Syn
{
    type Transport = SyncAddressSender<A>;

    /// Indicates if actor is still alive
    fn connected(tx: &Self::Transport) -> bool {
        tx.connected()
    }
}

impl<A: Actor, M> MessageDestination<A, M> for Syn
    where A: Handler<M>, A::Context: ToEnvelope<Self, A, M>,
          M: Message + Send + 'static, M::Result: Send,
{
    type Envelope = SyncEnvelope<A>;
    type Future = Request<Syn, A, M>;
    type Subscriber = SyncSubscriber<M>;
    type ResultSender = Sender<M::Result>;
    type ResultReceiver = Receiver<M::Result>;

    fn send(tx: &Self::Transport, msg: M) {
        let _ = tx.do_send(msg);
    }

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.try_send(msg, false)
    }

    fn call(tx: &Self::Transport, msg: M) -> Self::Future {
        match tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::Full(msg)) => Request::new(None, Some((tx.clone(), msg))),
            Err(SendError::Closed(_)) => Request::new(None, None),
        }
    }

    /// Get `Subscriber` for specific message type
    fn subscriber(tx: Self::Transport) -> SyncSubscriber<M> {
        SyncSubscriber{tx: tx.into_sender()}
    }
}

/// `SyncSubscriber` type allows to send one specific message to an actor.
///
/// You can get subscriber with `SyncAddress::into_subscriber::<M>()` method.
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub struct SyncSubscriber<M>
    where M: Message + Send + 'static, M::Result: Send,
{
    pub(crate) tx: Box<SyncSender<M>>
}

unsafe impl<M> Send for SyncSubscriber<M>
    where M: Message + Send + 'static, M::Result: Send {}

impl<M> SyncSubscriber<M>
    where M: Message + Send + 'static, M::Result: Send,
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
    where M: Message + Send + 'static, M::Result: Send,
{
    fn clone(&self) -> SyncSubscriber<M> {
        SyncSubscriber{tx: self.tx.boxed()}
    }
}
