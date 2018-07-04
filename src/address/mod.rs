use std::fmt;
use std::hash::{Hash, Hasher};

pub(crate) mod channel;
mod envelope;
mod message;
mod queue;

use actor::Actor;
use handler::{Handler, Message};

pub use self::envelope::{Envelope, EnvelopeProxy, ToEnvelope};
pub use self::message::{RecipientRequest, Request};

pub(crate) use self::channel::{AddressReceiver, AddressSenderProducer};
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

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
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

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
    pub fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.try_send(msg)
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

#[cfg(test)]
mod tests {
    use futures::Future;
    use prelude::*;

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

    impl actix::Handler<SetCounter> for ActorWithSmallMailBox {
        type Result = <SetCounter as Message>::Result;

        fn handle(
            &mut self, ping: SetCounter, _: &mut actix::Context<Self>,
        ) -> Self::Result {
            self.0.store(ping.0, Ordering::Relaxed);
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
            let send2 = addr.clone().send(SetCounter(2));
            assert!(!send2.rx_is_some());
            let send = send
                .join(send2)
                .map(|_| {
                    System::current().stop();
                })
                .map_err(|_| {
                    panic!("Message over limit should be delivered, but it is not!");
                });
            Arbiter::spawn(send);
        });

        assert_eq!(count.load(Ordering::Relaxed), 2);
    }
}
