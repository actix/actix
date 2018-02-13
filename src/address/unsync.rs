use futures::unsync::oneshot::{Receiver, Sender};

use actor::{Actor, AsyncContext};
use handler::{Handler, Message};

use super::{Request, Recipient, RecipientRequest};
use super::{ToEnvelope, UnsyncEnvelope, MessageEnvelope};
use super::{Destination, MessageDestination, MessageRecipient, SendError};
use super::unsync_channel::{UnsyncSender, UnsyncAddrSender};


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
    type ResultSender = Sender<M::Result>;
    type ResultReceiver = Receiver<M::Result>;

    fn send(tx: &Self::Transport, msg: M) {
        let _ = tx.do_send(msg);
    }

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.try_send(msg, false)
    }

    fn call(tx: &Self::Transport, msg: M) -> Request<Self, A, M> {
        match tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::Full(msg)) => Request::new(None, Some((tx.clone(), msg))),
            Err(SendError::Closed(_)) => Request::new(None, None),
        }
    }

    /// Get `Subscriber` for specific message type
    fn recipient(tx: Self::Transport) -> Recipient<Self, M> {
        Recipient::new(tx.into_sender())
    }
}

impl<M> MessageRecipient<M> for Unsync where M: Message + 'static
{
    type Envelope = MessageEnvelope<M>;
    type Transport = Box<UnsyncSender<M>>;
    type ResultReceiver = Receiver<M::Result>;

    fn send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.do_send(msg)
    }

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.try_send(msg)
    }

    fn call(tx: &Self::Transport, msg: M) -> RecipientRequest<Self, M> {
        match tx.send(msg) {
            Ok(rx) => RecipientRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                RecipientRequest::new(None, Some((tx.boxed(), msg))),
            Err(SendError::Closed(_)) =>
                RecipientRequest::new(None, None),
        }
    }

    fn clone(tx: &Self::Transport) -> Self::Transport {
        tx.boxed()
    }
}
