use std::fmt;

pub(crate) mod channel;
mod envelope;
mod message;
mod queue;

use actor::Actor;
use handler::{Handler, Message};

pub use self::envelope::{Envelope, EnvelopeProxy, ToEnvelope};
pub use self::message::{RecipientRequest, Request};

pub(crate) use self::channel::AddressReceiver;
use self::channel::{AddressSender, Sender};

pub enum SendError<T> {
    Full(T),
    Closed(T),
}

#[derive(Fail)]
/// Set of error that can occurred during message delivery process
pub enum MailboxError {
    #[fail(display = "Mailbox has closed")]
    Closed,
    #[fail(display = "Message delivery timed out")]
    Timeout,
}

impl<T> SendError<T> {
    pub fn into_inner(self) -> T {
        match self {
            SendError::Full(msg) | SendError::Closed(msg) => msg,
        }
    }
}

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SendError::Full(_) => write!(fmt, "SendError::Full(..)"),
            SendError::Closed(_) => write!(fmt, "SendError::Closed(..)"),
        }
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SendError::Full(_) => write!(fmt, "send failed because receiver is full"),
            SendError::Closed(_) => write!(fmt, "send failed because receiver is gone"),
        }
    }
}

impl fmt::Debug for MailboxError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "MailboxError({})", self)
    }
}

/// Address of the actor
pub struct Addr<A: Actor> {
    tx: AddressSender<A>,
}

impl<A: Actor> Addr<A> {
    pub fn new(tx: AddressSender<A>) -> Addr<A> {
        Addr { tx }
    }

    #[inline]
    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    #[inline]
    /// Send message unconditionally
    ///
    /// This method ignores actor's mailbox capacity, it silently fails if
    /// mailbox is closed.
    pub fn do_send<M>(&self, msg: M)
    where
        M: Message + Send,
        M::Result: Send,
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
    {
        let _ = self.tx.do_send(msg);
    }

    #[inline]
    /// Send asynchronous message and wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Future`
    /// object get dropped, message cancels.
    pub fn send<M>(&self, msg: M) -> Request<A, M>
    where
        M: Message + Send,
        M::Result: Send,
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
    {
        match self.tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::Full(msg)) => {
                Request::new(None, Some((self.tx.clone(), msg)))
            }
            Err(SendError::Closed(_)) => Request::new(None, None),
        }
    }

    /// Get `Recipient` for specific message type
    pub fn recipient<M: 'static>(self) -> Recipient<M>
    where
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
        M: Message + Send,
        M::Result: Send,
    {
        Recipient::new(Box::new(self.tx.clone()))
    }
}

impl<A: Actor> Clone for Addr<A> {
    fn clone(&self) -> Addr<A> {
        Addr {
            tx: self.tx.clone(),
        }
    }
}

/// `Recipient` type allows to send one specific message to an actor.
///
/// You can get recipient with `Addr<_, _>::recipient()` method.
/// It is possible to use `Clone::clone()` method to get cloned recipient.
pub struct Recipient<M: Message>
where
    M: Message + Send,
    M::Result: Send,
{
    tx: Box<Sender<M>>,
}

impl<M> Recipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    /// Create new recipient
    pub(crate) fn new(tx: Box<Sender<M>>) -> Recipient<M> {
        Recipient { tx }
    }

    /// Send message
    ///
    /// Deliver message even if recipient's mailbox is full
    pub fn do_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.do_send(msg)
    }

    /// Send message and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Request`
    /// object get dropped, message cancels.
    pub fn send(&self, msg: M) -> RecipientRequest<M> {
        match self.tx.send(msg) {
            Ok(rx) => RecipientRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) => {
                RecipientRequest::new(None, Some((self.tx.boxed(), msg)))
            }
            Err(SendError::Closed(_)) => RecipientRequest::new(None, None),
        }
    }
}

impl<M> Clone for Recipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    fn clone(&self) -> Recipient<M> {
        Recipient {
            tx: self.tx.boxed(),
        }
    }
}
