use std::fmt;
use std::marker::PhantomData;
use futures::Future;

mod envelope;
mod queue;
mod message;

mod sync;
pub(crate) mod sync_channel;

mod unsync;
mod unsync_channel;

use actor::{Actor, AsyncContext};
use handler::{Handler, Message};

pub use self::message::Request;
pub use self::envelope::{EnvelopeProxy, ToEnvelope, SyncEnvelope, UnsyncEnvelope,
                         MessageEnvelope, SyncMessageEnvelope};

pub use self::sync::{Syn, SyncRecipientRequest};
pub use self::unsync::{Unsync, UnsyncRecipientRequest};
pub(crate) use self::sync_channel::SyncAddressReceiver;
pub(crate) use self::unsync_channel::UnsyncAddrReceiver;


pub enum SendError<T> {
    Full(T),
    Closed(T),
}

#[derive(Fail)]
/// Set of error that can occurred during message delivery process
pub enum MailboxError {
    #[fail(display="Mailbox has closed")]
    Closed,
    #[fail(display="Message delivery timed out")]
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

/// Trait give access to actor's address
pub trait ActorAddress<A, T> where A: Actor {
    /// Returns actor's address for specific context
    fn get(ctx: &mut A::Context) -> T;
}

impl<A> ActorAddress<A, Addr<Unsync, A>> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> Addr<Unsync, A> {
        ctx.unsync_address()
    }
}

impl<A> ActorAddress<A, Addr<Syn, A>> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> Addr<Syn, A> {
        ctx.sync_address()
    }
}

impl<A> ActorAddress<A, (Addr<Unsync, A>, Addr<Syn, A>)> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> (Addr<Unsync, A>, Addr<Syn, A>) {
        (ctx.unsync_address(), ctx.sync_address())
    }
}

impl<A> ActorAddress<A, ()> for A where A: Actor {
    fn get(_: &mut A::Context) -> () {
        ()
    }
}

pub trait Destination<A>: Sized {
    type Transport: Clone;

    /// Indicates if destination is still alive
    fn connected(tx: &Self::Transport) -> bool;
}

#[allow(unused_variables)]
pub trait MessageDestination<A, M>: Destination<A>
    where A: Handler<M>, A::Context: ToEnvelope<Self, A, M>,
          M: Message + 'static,
          Self::Transport: MessageDestinationTransport<Self, A, M>,
{
    type Envelope;
    type ResultSender;
    type ResultReceiver: Future<Item=M::Result>;

    /// Send message unconditionally
    fn do_send(tx: &Self::Transport, msg: M);

    /// Try send message
    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>>;

    /// Send asynchronous message and wait for response.
    fn send(tx: &Self::Transport, msg: M) -> Request<Self, A, M>;

    /// Get recipient for specific message type.
    fn recipient(tx: <Self as Destination<A>>::Transport) -> Recipient<Self, M>
        where Self: MessageRecipient<M>;
}

pub trait MessageDestinationTransport<T: MessageDestination<A, M>, A, M>
    where M: Message + 'static,
          A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
          T::Transport: MessageDestinationTransport<T, A, M>,
{
    fn send(&self, msg: M) -> Result<T::ResultReceiver, SendError<M>>;
}

#[allow(unused_variables)]
pub trait MessageRecipient<M>: Sized where M: Message + 'static
{
    type Envelope: From<M>;
    type Transport;

    type SendError;
    type MailboxError;
    type Request: Future<Item=M::Result, Error=Self::MailboxError>;

    /// Send message unconditionally
    ///
    /// Deliver message even if recipient's mailbox is full
    fn do_send(tx: &Self::Transport, msg: M) -> Result<(), Self::SendError>;

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), Self::SendError>;

    /// Send asynchronous message and wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Receiver`
    /// object get dropped, message cancels.
    fn send(tx: &Self::Transport, msg: M) -> Self::Request;

    /// Clone transport
    fn clone(tx: &Self::Transport) -> Self::Transport;
}

/// Address of the actor
pub struct Addr<T: Destination<A>, A> {
    tx: T::Transport,
    act: PhantomData<A>,
}

unsafe impl<A: Actor> Send for Addr<Syn, A> {}
unsafe impl<A: Actor> Sync for Addr<Syn, A> {}

impl<T: Destination<A>, A> Addr<T, A> {
    pub fn new(tx: T::Transport) -> Addr<T, A> {
        Addr{tx: tx, act: PhantomData}
    }

    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        T::connected(&self.tx)
    }

    /// Sendm message unconditionally
    ///
    /// This method ignores actor's mailbox capacity, it silently fails if mailbox is closed.
    pub fn do_send<M>(&self, msg: M)
        where T: MessageDestination<A, M>,
              T::Transport: MessageDestinationTransport<T, A, M>,
              M: Message + 'static,
              A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
    {
        T::do_send(&self.tx, msg)
    }

    /// Send asynchronous message and wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Receiver`
    /// object get dropped, message cancels.
    pub fn send<M>(&self, msg: M) -> Request<T, A, M>
        where T: MessageDestination<A, M>,
              T::Transport: MessageDestinationTransport<T, A, M>,
              M: Message + 'static,
              A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
    {
        T::send(&self.tx, msg)
    }

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where T: MessageDestination<A, M>,
              T::Transport: MessageDestinationTransport<T, A, M>,
              M: Message + 'static,
              A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
    {
        T::try_send(&self.tx, msg)
    }

    /// Get `Recipient` for specific message type
    pub fn recipient<M>(self) -> Recipient<T, M>
        where T: MessageDestination<A, M> + MessageRecipient<M>,
              A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
              <T as Destination<A>>::Transport: MessageDestinationTransport<T, A, M>,
              M: Message + 'static,
    {
        T::recipient(self.tx)
    }
}

impl<T: Destination<A>, A> Clone for Addr<T, A> {
    fn clone(&self) -> Addr<T, A> {
        Addr{tx: self.tx.clone(), act: PhantomData}
    }
}

/// `Subscriber` type allows to send one specific message to an actor.
///
/// You can get subscriber with `Addr<_, _>::subscriber()` method.
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub struct Recipient<T: MessageRecipient<M>, M: Message + 'static> {
    tx: T::Transport,
    msg: PhantomData<M>,
}

unsafe impl<M> Send for Recipient<Syn, M>
    where M: Message + Send + 'static, M::Result: Send {}
unsafe impl<M> Sync for Recipient<Syn, M>
    where M: Message + Send + 'static, M::Result: Send {}

impl<T, M> Recipient<T, M>
    where T: MessageRecipient<M>, M: Message + 'static
{
    /// Create new subscriber
    pub fn new(tx: T::Transport) -> Recipient<T, M> {
        Recipient{tx: tx, msg: PhantomData}
    }

    /// Send message
    ///
    /// Deliver message even if recipient's mailbox is full
    pub fn do_send(&self, msg: M) -> Result<(), T::SendError> {
        T::do_send(&self.tx, msg)
    }

    /// Try send message
    ///
    /// This method fails if recipient's mailbox is full or closed. This method
    /// register current task in receivers queue.
    pub fn try_send(&self, msg: M) -> Result<(), T::SendError> {
        T::try_send(&self.tx, msg)
    }

    /// Send message and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Request`
    /// object get dropped, message cancels.
    pub fn send(&self, msg: M) -> T::Request {
        T::send(&self.tx, msg)
    }
}

impl<T, M> Clone for Recipient<T, M>
    where T: MessageRecipient<M>, M: Message + 'static
{
    fn clone(&self) -> Recipient<T, M> {
        Recipient{tx: T::clone(&self.tx), msg: PhantomData}
    }
}
