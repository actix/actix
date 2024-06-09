use std::{
    error, fmt,
    hash::{Hash, Hasher},
};

pub(crate) mod channel;
mod envelope;
mod message;
mod queue;

pub(crate) use self::channel::{AddressReceiver, AddressSenderProducer};
use self::channel::{AddressSender, Sender, WeakAddressSender, WeakSender};
pub use self::{
    envelope::{Envelope, EnvelopeProxy, ToEnvelope},
    message::{RecipientRequest, Request},
};
use crate::{
    actor::Actor,
    handler::{Handler, Message},
};

pub enum SendError<T> {
    Full(T),
    Closed(T),
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// The errors that can occur during the message delivery process.
pub enum MailboxError {
    Closed,
    Timeout,
}

impl fmt::Debug for MailboxError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "MailboxError({})", self)
    }
}

impl fmt::Display for MailboxError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MailboxError::Closed => write!(fmt, "Mailbox has closed"),
            MailboxError::Timeout => write!(fmt, "Message delivery timed out"),
        }
    }
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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            SendError::Full(_) => write!(fmt, "SendError::Full(..)"),
            SendError::Closed(_) => write!(fmt, "SendError::Closed(..)"),
        }
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            SendError::Full(_) => write!(fmt, "send failed because receiver is full"),
            SendError::Closed(_) => write!(fmt, "send failed because receiver is gone"),
        }
    }
}

/// The address of an actor.
pub struct Addr<A: Actor> {
    tx: AddressSender<A>,
}

impl<A: Actor> Addr<A> {
    pub fn new(tx: AddressSender<A>) -> Addr<A> {
        Addr { tx }
    }

    /// Returns whether the actor is still alive.
    #[inline]
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    /// Sends a message unconditionally, ignoring any potential errors.
    ///
    /// The message is always queued, even if the mailbox for the receiver is full. If the mailbox
    /// is closed, the message is silently dropped.
    #[inline]
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

    /// Sends an asynchronous message and waits for a response.
    ///
    /// The communication channel to the actor is bounded. If the returned request future gets
    /// dropped, the message is cancelled.
    #[inline]
    pub fn send<M>(&self, msg: M) -> Request<A, M>
    where
        M: Message + Send + 'static,
        M::Result: Send,
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
    {
        match self.tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::Full(msg)) => Request::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) => Request::new(None, None),
        }
    }

    /// Returns the [`Recipient`] for a specific message type.
    pub fn recipient<M>(self) -> Recipient<M>
    where
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
        M: Message + Send + 'static,
        M::Result: Send,
    {
        self.into()
    }

    /// Returns a downgraded [`WeakAddr`].
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

impl<A: Actor> fmt::Debug for Addr<A> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Addr").field("tx", &self.tx).finish()
    }
}

/// A weakly referenced counterpart to `Addr<A>`.
pub struct WeakAddr<A: Actor> {
    wtx: WeakAddressSender<A>,
}

impl<A: Actor> WeakAddr<A> {
    /// Attempts to upgrade the [`WeakAddr<A>`] pointer to an [`Addr<A>`].
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

    pub fn recipient<M>(self) -> WeakRecipient<M>
    where
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
        M: Message + Send + 'static,
        M::Result: Send,
    {
        self.into()
    }
}

impl<A: Actor> Clone for WeakAddr<A> {
    fn clone(&self) -> WeakAddr<A> {
        WeakAddr {
            wtx: self.wtx.clone(),
        }
    }
}

impl<A: Actor> fmt::Debug for WeakAddr<A> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("WeakAddr")
            .field("wtx", &self.wtx)
            .finish()
    }
}

impl<A: Actor> PartialEq for WeakAddr<A> {
    fn eq(&self, other: &Self) -> bool {
        self.wtx == other.wtx
    }
}

impl<A: Actor> std::cmp::Eq for WeakAddr<A> {}

/// The [`Recipient`] type allows to send one specific message to an actor.
///
/// You can get a recipient using the `Addr::recipient()` method. It is possible
/// to use the `Clone::clone()` method to get a cloned recipient.
pub struct Recipient<M: Message>
where
    M: Message + Send,
    M::Result: Send,
{
    tx: Box<dyn Sender<M> + Sync>,
}

impl<M> Recipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    /// Creates a new recipient.
    pub(crate) fn new(tx: Box<dyn Sender<M> + Sync>) -> Recipient<M> {
        Recipient { tx }
    }

    /// Sends a message.
    ///
    /// The message is always queued, even if the mailbox for the receiver is full. If the mailbox
    /// is closed, the message is silently dropped.
    pub fn do_send(&self, msg: M) {
        let _ = self.tx.do_send(msg);
    }

    /// Attempts to send a message.
    ///
    /// This method fails if the actor's mailbox is full or closed. This method registers the
    /// current task in the receivers queue.
    pub fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.try_send(msg)
    }

    /// Sends a message and asynchronously wait for a response.
    ///
    /// The communication channel to the actor is bounded. If the returned `RecipientRequest` object
    /// gets dropped, the message is cancelled.
    pub fn send(&self, msg: M) -> RecipientRequest<M> {
        match self.tx.send(msg) {
            Ok(rx) => RecipientRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) => RecipientRequest::new(None, Some((self.tx.boxed(), msg))),
            Err(SendError::Closed(_)) => RecipientRequest::new(None, None),
        }
    }

    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    /// Returns a downgraded `WeakRecipient`
    pub fn downgrade(&self) -> WeakRecipient<M> {
        WeakRecipient {
            wtx: self.tx.downgrade(),
        }
    }
}

impl<A: Actor, M: Message + Send + 'static> From<Addr<A>> for Recipient<M>
where
    A: Handler<M>,
    M::Result: Send,
    A::Context: ToEnvelope<A, M>,
{
    fn from(addr: Addr<A>) -> Self {
        Recipient::new(Box::new(addr.tx))
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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Recipient {{ /* omitted */ }}")
    }
}

/// A weakly referenced counterpart to `Recipient<M>`
pub struct WeakRecipient<M: Message>
where
    M: Message + Send,
    M::Result: Send,
{
    wtx: Box<dyn WeakSender<M> + Sync>,
}

impl<M> fmt::Debug for WeakRecipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "WeakRecipient {{ /* omitted */ }}")
    }
}

impl<M> Clone for WeakRecipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    fn clone(&self) -> Self {
        Self {
            wtx: self.wtx.boxed(),
        }
    }
}

impl<M> From<Recipient<M>> for WeakRecipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    fn from(recipient: Recipient<M>) -> Self {
        recipient.downgrade()
    }
}

impl<M> WeakRecipient<M>
where
    M: Message + Send,
    M::Result: Send,
{
    pub(crate) fn new(wtx: Box<dyn WeakSender<M> + Sync>) -> WeakRecipient<M> {
        WeakRecipient { wtx }
    }

    /// Attempts to upgrade the `WeakRecipient<M>` pointer to an `Recipient<M>`, similar to `WeakAddr<A>`
    pub fn upgrade(&self) -> Option<Recipient<M>> {
        self.wtx.upgrade().map(Recipient::new)
    }
}

impl<A: Actor, M: Message + Send + 'static> From<Addr<A>> for WeakRecipient<M>
where
    A: Handler<M>,
    M::Result: Send,
    A::Context: ToEnvelope<A, M>,
{
    fn from(addr: Addr<A>) -> WeakRecipient<M> {
        addr.downgrade().recipient()
    }
}

impl<A: Actor, M: Message + Send + 'static> From<WeakAddr<A>> for WeakRecipient<M>
where
    A: Handler<M>,
    M::Result: Send,
    A::Context: ToEnvelope<A, M>,
{
    fn from(addr: WeakAddr<A>) -> WeakRecipient<M> {
        WeakRecipient::new(Box::new(addr.wtx))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::prelude::*;

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

        let sys = System::new();

        sys.block_on(async move {
            //Actor::started gets called after we relinquish
            //control to event loop so we just set it ourself.
            let addr = ActorWithSmallMailBox::create(|ctx| {
                ctx.set_mailbox_capacity(1);
                ActorWithSmallMailBox(count2)
            });

            let fut = async move {
                let send = addr.clone().send(SetCounter(1));
                assert!(send.rx_is_some());
                let addr2 = addr.clone();
                let send2 = addr2.send(SetCounter(2));
                assert!(send2.rx_is_some());
                let send3 = addr2.send(SetCounter(3));
                assert!(!send3.rx_is_some());

                let _ = send.await;
                let _ = send2.await;
                let _ = send3.await;

                System::current().stop();
            };
            actix_rt::spawn(fut);
        });

        // run til system stop
        sys.run().unwrap();

        assert_eq!(count.load(Ordering::Relaxed), 3);
    }
}
