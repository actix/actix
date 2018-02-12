use futures::unsync::oneshot::{Receiver, Sender};

use actor::{Actor, AsyncContext};
use handler::{Handler, Message};

use super::ToEnvelope;
use super::message::Request;
use super::envelope::UnsyncEnvelope;
use super::unsync_channel::{UnsyncSender, UnsyncAddrSender};
use super::UnsyncSubscriberRequest;
use super::{Destination, MessageDestination, SendError};


/// Unsync destination of the actor
///
/// Actor has to run in the same thread as owner of the address.
pub struct Unsync;

impl<A: Actor> Destination<A> for Unsync
    where A::Context: AsyncContext<A>
{
    type Transport = UnsyncAddrSender<A>;

    /// Indicates if actor is still alive
    fn connected(tx: &Self::Transport) -> bool {
        tx.connected()
    }
}

impl<A, M> MessageDestination<A, M> for Unsync
    where M: Message + 'static,
          A: Handler<M>, A::Context: AsyncContext<A> + ToEnvelope<Self, A, M>
{
    type Envelope = UnsyncEnvelope<A>;
    type Future = Request<Unsync, A, M>;
    type Subscriber = Subscriber<M>;
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
    fn subscriber(tx: Self::Transport) -> Subscriber<M> {
        Subscriber(tx.into_sender())
    }
}

/// `Subscriber` type allows to send one specific message to an actor.
///
/// You can get subscriber with `Address::into_subscriber::<M>()` method.
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub struct Subscriber<M: Message+'static>(pub(crate) Box<UnsyncSender<M>>);

impl<M: Message + 'static> Subscriber<M> {
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
    pub fn call(&self, msg: M) -> UnsyncSubscriberRequest<M> {
        match self.0.send(msg) {
            Ok(rx) => UnsyncSubscriberRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                UnsyncSubscriberRequest::new(None, Some((self.0.boxed(), msg))),
            Err(SendError::Closed(_)) =>
                UnsyncSubscriberRequest::new(None, None),
        }
    }
}

impl<M: Message + 'static> Clone for Subscriber<M> {
    fn clone(&self) -> Subscriber<M> {
        Subscriber(self.0.boxed())
    }
}
