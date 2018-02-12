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

pub use self::envelope::{EnvelopeProxy, ToEnvelope, RemoteEnvelope};

pub use self::unsync::{Unsync, Subscriber};
pub(crate) use self::unsync_channel::UnsyncAddrReceiver;
pub use self::unsync_message::UnsyncSubscriberRequest;

pub use self::sync::{Syn, SyncSubscriber};
pub use self::message::{Request, RequestFut, SyncSubscriberRequest};
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

impl<A> ActorAddress<A, Addr<Unsync<A>>> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> Addr<Unsync<A>> {
        ctx.unsync_address()
    }
}

impl<A> ActorAddress<A, Addr<Syn<A>>> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> Addr<Syn<A>> {
        ctx.sync_address()
    }
}

impl<A> ActorAddress<A, (Addr<Unsync<A>>, Addr<Syn<A>>)> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> (Addr<Unsync<A>>, Addr<Syn<A>>) {
        (ctx.unsync_address(), ctx.sync_address())
    }
}

impl<A> ActorAddress<A, ()> for A where A: Actor {
    fn get(_: &mut A::Context) -> () {
        ()
    }
}

pub trait DestinationSender<T: MessageDestination<M>, M>
    where M: Message + 'static,
          T::Actor: Handler<M>, <T::Actor as Actor>::Context: ToEnvelope<T, M>,
          T::Transport: DestinationSender<T, M>,
{
    fn send(&self, msg: M) -> Result<T::ResultReceiver, SendError<M>>;
}

pub trait Destination: Sized {
    type Actor;
    type Transport: Clone;

    /// Indicates if destination is still alive
    fn connected(tx: &Self::Transport) -> bool;
}

#[allow(unused_variables)]
pub trait MessageDestination<M>: Destination
    where Self::Actor: Handler<M>, <Self::Actor as Actor>::Context: ToEnvelope<Self, M>,
          M: Message + 'static,
          Self::Transport: DestinationSender<Self, M>,
{
    type Future;
    type Subscriber;
    type ResultSender;
    type ResultReceiver: Future<Item=M::Result>;

    fn send(tx: &Self::Transport, msg: M);

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>>;

    fn call_fut(tx: &Self::Transport, msg: M) -> Self::Future;

    fn subscriber(tx: Self::Transport) -> Self::Subscriber;
}

#[allow(unused_variables)]
pub trait ActorMessageDestination<M, B>: MessageDestination<M>
    where Self::Actor: Handler<M>,
          M: Message + 'static,
          B: Actor,
          Self::Transport: DestinationSender<Self, M>,
          <Self::Actor as Actor>::Context: ToEnvelope<Self, M>,
{
    type ActFuture;

    fn call(tx: &Self::Transport, act: &B, msg: M) -> Self::ActFuture;
}

/// Address of the actor
pub struct Addr<T: Destination> {
    tx: T::Transport,
}

impl<T: Destination> Addr<T> {
    pub fn new(tx: T::Transport) -> Addr<T> {
        Addr{tx: tx}
    }

    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        T::connected(&self.tx)
    }

    /// Send message `M` to the actor `A`
    ///
    /// This method ignores receiver capacity, it silently fails if mailbox is closed.
    pub fn send<M>(&self, msg: M)
        where T: MessageDestination<M>, T::Actor: Handler<M>, M: Message + 'static,
              T::Transport: DestinationSender<T, M>,
             <T::Actor as Actor>::Context: ToEnvelope<T, M>,
    {
        T::send(&self.tx, msg)
    }

    /// Try to send message `M` to the actor `A`
    ///
    /// This function fails if receiver is full or closed.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where T: MessageDestination<M>, T::Actor: Handler<M>, M: Message + 'static,
              T::Transport: DestinationSender<T, M>,
             <T::Actor as Actor>::Context: ToEnvelope<T, M>,
    {
        T::try_send(&self.tx, msg)
    }

    /// Send message to the actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `Future` object get dropped, message cancels.
    pub fn call_fut<M>(&self, msg: M) -> T::Future
        where T: MessageDestination<M>, T::Actor: Handler<M>, M: Message + 'static,
              T::Transport: DestinationSender<T, M>,
             <T::Actor as Actor>::Context: ToEnvelope<T, M>,
    {
        T::call_fut(&self.tx, msg)
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `Future` object get dropped, message cancels.
    pub fn call<M, B>(&self, act: &B, msg: M) -> T::ActFuture
        where T: ActorMessageDestination<M, B>,
              T::Actor: Handler<M>,
              T::Transport: DestinationSender<T, M>,
              M: Message + 'static,
              B: Actor,
             <T::Actor as Actor>::Context: ToEnvelope<T, M>,
    {
        T::call(&self.tx, act, msg)
    }

    /// Get `Subscriber` for specific message type
    pub fn subscriber<M>(self) -> T::Subscriber
        where T: MessageDestination<M>,
              T::Actor: Handler<M>,
             <T::Actor as Actor>::Context: ToEnvelope<T, M>,
              T::Transport: DestinationSender<T, M>,
              M: Message + 'static,
    {
        T::subscriber(self.tx)
    }
}

impl<T: Destination> Clone for Addr<T> {
    fn clone(&self) -> Addr<T> {
        Addr{tx: self.tx.clone()}
    }
}
