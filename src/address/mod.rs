use derive_more::Display;
use std::hash::{Hash, Hasher};
use std::{error, fmt};

pub(crate) mod channel;
mod envelope;
mod message;
mod queue;

use crate::actor::Actor;
use crate::handler::{Handler, Message};

pub use self::envelope::{Envelope, EnvelopeProxy, ToEnvelope};
pub use self::message::{RecipientRequest, Request};

pub(crate) use self::channel::{AddressReceiver, AddressSenderProducer};
use self::channel::{AddressSender, Sender, WeakAddressSender};

pub enum SendError<T> {
    Full(T),
    Closed(T),
}

#[derive(Display)]
/// The errors that can occur during the message delivery process.
pub enum MailboxError {
    #[display(fmt = "Mailbox has closed")]
    Closed,
    #[display(fmt = "Message delivery timed out")]
    Timeout,
}

impl error::Error for MailboxError {}

impl<T> SendError<T> {
    pub fn into_inner(self) -> T {
        match self {
            SendError::Full(msg) | SendError::Closed(msg) => msg,
        }
    }
}

impl<T> error::Error for SendError<T> {}

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

/// The address of an actor.
#[derive(Debug)]
pub struct Addr<A: Actor> {
    tx: AddressSender<A>,
}

impl<A: Actor> Addr<A> {
    pub fn new(tx: AddressSender<A>) -> Addr<A> {
        Addr { tx }
    }

    #[inline]
    /// Returns whether the actor is still alive.
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    #[inline]
    /// Sends a message unconditionally, ignoring any potential errors.
    ///
    /// The message is always queued, even if the mailbox for the receiver is full.
    /// If the mailbox is closed, the message is silently dropped.
    pub fn do_send<M>(&self, msg: M)
    where
        M: Message + Send,
        M::Result: Send,
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
    {
        let _ = self.tx.do_send(msg);
    }

    /// Tries to send a message.
    ///
    /// This method fails if actor's mailbox is full or closed. This
    /// method registers the current task in the receiver's queue.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
    where
        M: Message + Send + 'static,
        M::Result: Send,
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
    {
        self.tx.try_send(msg, true)
    }

    #[inline]
    /// Sends an asynchronous message and waits for a response.
    ///
    /// The communication channel to the actor is bounded. If the
    /// returned `Future` object gets dropped, the message is
    /// cancelled.
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

    /// Returns the `Recipient` for a specific message type.
    pub fn recipient<M: 'static>(self) -> Recipient<M>
    where
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
        M: Message + Send,
        M::Result: Send,
    {
        self.into()
    }

    /// Returns a downgraded `WeakAddr`.
    pub fn downgrade(&self) -> WeakAddr<A> {
        WeakAddr {
            wtx: self.tx.downgrade(),
        }
    }
}

impl<A: Actor> Clone for Addr<A> {
    fn clone(&self) -> Addr<A> {
        Addr {
            tx: self.tx.clone(),
        }
    }
}

impl<A: Actor> PartialEq for Addr<A> {
    fn eq(&self, other: &Self) -> bool {
        self.tx == other.tx
    }
}

impl<A: Actor> Eq for Addr<A> {}

impl<A: Actor> Hash for Addr<A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.tx.hash(state)
    }
}

/// A weakly referenced counterpart to `Addr<A>`.
#[derive(Debug)]
pub struct WeakAddr<A: Actor> {
    wtx: WeakAddressSender<A>,
}

impl<A: Actor> WeakAddr<A> {
    /// Attempts to upgrade the `WeakAddr<A>` pointer to an `Addr<A>`.
    ///
    /// Returns `None` if the actor has since been dropped or the
    /// underlying address is disconnected.
    pub fn upgrade(&self) -> Option<Addr<A>> {
        match self.wtx.upgrade() {
            Some(tx) => {
                if tx.connected() {
                    Some(Addr::new(tx))
                } else {
                    None
                }
            }
            None => None,
        }
    }
}

/// The `Recipient` type allows to send one specific message to an
/// actor.
///
/// You can get a recipient using the `Addr::recipient()` method. It
/// is possible to use the `Clone::clone()` method to get a cloned
/// recipient.
pub struct Recipient<M: Message>
where
    M: Message + Send,
    M::Result: Send,
{
    tx: Box<dyn Sender<M>>,
}

impl<M> Recipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    /// Creates a new recipient.
    pub(crate) fn new(tx: Box<dyn Sender<M>>) -> Recipient<M> {
        Recipient { tx }
    }

    /// Sends a message.
    ///
    /// Deliver the message even if the recipient's mailbox is full.
    pub fn do_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.do_send(msg)
    }

    /// Attempts to send a message.
    ///
    /// This method fails if the actor's mailbox is full or
    /// closed. This method registers the current task in the
    /// receivers queue.
    pub fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.try_send(msg)
    }

    /// Sends a message and asynchronously wait for a response.
    ///
    /// The communication channel to the actor is bounded. If the
    /// returned `Request` object gets dropped, the message is
    /// cancelled.
    pub fn send(&self, msg: M) -> RecipientRequest<M> {
        match self.tx.send(msg) {
            Ok(rx) => RecipientRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) => {
                RecipientRequest::new(None, Some((self.tx.boxed(), msg)))
            }
            Err(SendError::Closed(_)) => RecipientRequest::new(None, None),
        }
    }

    pub fn connected(&self) -> bool {
        self.tx.connected()
    }
}

impl<A: Actor, M: Message + Send + 'static> Into<Recipient<M>> for Addr<A>
where
    A: Handler<M>,
    M::Result: Send,
    A::Context: ToEnvelope<A, M>,
{
    fn into(self) -> Recipient<M> {
        Recipient::new(Box::new(self.tx))
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

impl<M> PartialEq for Recipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    fn eq(&self, other: &Self) -> bool {
        self.tx.hash() == other.tx.hash()
    }
}

impl<M> Eq for Recipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
}

impl<M> Hash for Recipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.tx.hash().hash(state)
    }
}

impl<M> fmt::Debug for Recipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Recipient {{ /* omitted */ }}")
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use futures::Future;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct ActorWithSmallMailBox(Arc<AtomicUsize>);

    impl Actor for ActorWithSmallMailBox {
        type Context = Context<Self>;

        fn started(&mut self, ctx: &mut Self::Context) {
            ctx.set_mailbox_capacity(1);
        }
    }

    pub struct SetCounter(usize);
    impl Message for SetCounter {
        type Result = ();
    }

    impl Handler<SetCounter> for ActorWithSmallMailBox {
        type Result = <SetCounter as Message>::Result;

        fn handle(&mut self, ping: SetCounter, _: &mut Context<Self>) -> Self::Result {
            self.0.store(ping.0, Ordering::Relaxed)
        }
    }

    #[test]
    fn test_send_over_limit() {
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = Arc::clone(&count);

        System::run(move || {
            //Actor::started gets called after we relinquish
            //control to event loop so we just set it ourself.
            let addr = ActorWithSmallMailBox::create(|ctx| {
                ctx.set_mailbox_capacity(1);
                ActorWithSmallMailBox(count2)
            });
            //Use clone to make sure that regardless of how many messages
            //are cloned capacity will be taken into account.
            let send = addr.clone().send(SetCounter(1));
            assert!(send.rx_is_some());
            let addr2 = addr.clone();
            let send2 = addr2.send(SetCounter(2));
            assert!(send2.rx_is_some());
            let send3 = addr2.send(SetCounter(3));
            assert!(!send3.rx_is_some());
            let send = send
                .join(send2)
                .join(send3)
                .map(|_| {
                    System::current().stop();
                })
                .map_err(|_| {
                    panic!("Message over limit should be delivered, but it is not!");
                });
            Arbiter::spawn(send);
        })
        .unwrap();

        assert_eq!(count.load(Ordering::Relaxed), 3);
    }
}
