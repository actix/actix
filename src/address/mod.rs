use std::fmt;
use futures::Future;

mod envelope;
mod queue;
mod message;

mod sync;
pub(crate) mod sync_channel;

mod unsync;
mod unsync_channel;
mod unsync_message;

use actor::{Actor, AsyncContext};
use handler::{Handler, Message};

pub use self::envelope::{EnvelopeProxy, ToEnvelope, SyncEnvelope};

pub use self::unsync::{Unsync, Subscriber};
pub(crate) use self::unsync_channel::UnsyncAddrReceiver;
pub use self::unsync_message::UnsyncSubscriberRequest;

pub use self::sync::{Syn, SyncSubscriber};
pub use self::message::{Request, SyncSubscriberRequest};
pub(crate) use self::sync_channel::SyncAddressReceiver;


pub enum SendError<T> {
    Full(T),
    Closed(T),
}

#[derive(Fail)]
/// Set of error that can occure during message delivery process
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

pub trait DestinationSender<T: MessageDestination<A, M>, A, M>
    where M: Message + 'static,
          A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
          T::Transport: DestinationSender<T, A, M>,
{
    fn send(&self, msg: M) -> Result<T::ResultReceiver, SendError<M>>;
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
          Self::Transport: DestinationSender<Self, A, M>,
{
    type Future;
    type Envelope; //: EnvelopeProxy<Actor=Self::Actor>;
    type Subscriber;
    type ResultSender;
    type ResultReceiver: Future<Item=M::Result>;

    fn send(tx: &Self::Transport, msg: M);

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>>;

    fn call(tx: &Self::Transport, msg: M) -> Self::Future;

    fn subscriber(tx: Self::Transport) -> Self::Subscriber;
}

use std::marker::PhantomData;

/// Address of the actor
pub struct Addr<T: Destination<A>, A> {
    tx: T::Transport,
    act: PhantomData<A>,
}

unsafe impl<A: Actor> Send for Addr<Syn, A> {}

impl<T: Destination<A>, A> Addr<T, A> {
    pub fn new(tx: T::Transport) -> Addr<T, A> {
        Addr{tx: tx, act: PhantomData}
    }

    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        T::connected(&self.tx)
    }

    /// Send message `M` to the actor `A`
    ///
    /// This method ignores receiver capacity, it silently fails if mailbox is closed.
    pub fn send<M>(&self, msg: M)
        where T: MessageDestination<A, M>,
              M: Message + 'static,
              T::Transport: DestinationSender<T, A, M>,
              A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
    {
        T::send(&self.tx, msg)
    }

    /// Try to send message `M` to the actor `A`
    ///
    /// This function fails if receiver is full or closed.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where T: MessageDestination<A, M>,
              M: Message + 'static,
              T::Transport: DestinationSender<T, A, M>,
              A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
    {
        T::try_send(&self.tx, msg)
    }

    /// Send message to the actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `Future` object get dropped, message cancels.
    pub fn call<M>(&self, msg: M) -> T::Future
        where T: MessageDestination<A, M>,
              T::Transport: DestinationSender<T, A, M>,
              M: Message + 'static,
              A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
    {
        T::call(&self.tx, msg)
    }

    /// Get `Subscriber` for specific message type
    pub fn subscriber<M>(self) -> T::Subscriber
        where T: MessageDestination<A, M>,
              A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
              T::Transport: DestinationSender<T, A, M>,
              M: Message + 'static,
    {
        T::subscriber(self.tx)
    }
}

impl<T: Destination<A>, A> Clone for Addr<T, A> {
    fn clone(&self) -> Addr<T, A> {
        Addr{tx: self.tx.clone(), act: PhantomData}
    }
}
