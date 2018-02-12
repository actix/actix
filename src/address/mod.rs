use std::fmt;

mod envelope;
mod queue;
pub(crate) mod sync_channel;
mod sync_address;
mod sync_message;

mod unsync;
mod unsync_channel;
mod unsync_message;

use actor::{Actor, AsyncContext};
use handler::{Handler, ResponseType};

pub use self::envelope::{Envelope, EnvelopeProxy, ToEnvelope, RemoteEnvelope};

pub use self::unsync::{Unsync, Subscriber};
pub(crate) use self::unsync_channel::UnsyncAddrReceiver;
pub use self::unsync_message::{UnsyncRequest, UnsyncFutRequest, UnsyncSubscriberRequest};

pub use self::sync_address::{SyncAddress, SyncSubscriber};
pub use self::sync_message::{Request, RequestFut, SyncSubscriberRequest};
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

impl<A> ActorAddress<A, SyncAddress<A>> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> SyncAddress<A> {
        ctx.sync_address()
    }
}

impl<A> ActorAddress<A, (Addr<Unsync<A>>, SyncAddress<A>)> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> (Addr<Unsync<A>>, SyncAddress<A>) {
        (ctx.unsync_address(), ctx.sync_address())
    }
}

impl<A> ActorAddress<A, ()> for A where A: Actor {
    fn get(_: &mut A::Context) -> () {
        ()
    }
}

pub trait Destination: Sized {
    type Actor;
    type Transport: Clone;

    /// Indicates if destination is still alive
    fn connected(tx: &Self::Transport) -> bool;
}

#[allow(unused_variables)]
pub trait MessageDestination<M>: Destination
    where Self::Actor: Handler<M>,
          M: ResponseType + 'static,
{
    type Future;
    type ResultTransport;

    fn send(tx: &Self::Transport, msg: M);

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>>;

    fn call_fut(tx: &Self::Transport, msg: M) -> Self::Future;

    fn subscriber(tx: Self::Transport) -> Subscriber<M>;
}

#[allow(unused_variables)]
pub trait ActorMessageDestination<M, B>: MessageDestination<M>
    where Self::Actor: Handler<M>,
          M: ResponseType + 'static,
          B: Actor,
{
    type ActFuture;

    fn call(tx: &Self::Transport, act: &B, msg: M) -> Self::ActFuture;
}

pub trait ToEnv<T: MessageDestination<M>, M: ResponseType + 'static>
    where T::Actor: Actor + Handler<M>
{
    /// Pack message into suitable envelope
    fn pack(msg: M, tx: Option<T::ResultTransport>) -> T;
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
        where T: MessageDestination<M>, T::Actor: Handler<M>, M: ResponseType + 'static,
             <T::Actor as Actor>::Context: ToEnv<T, M>,
    {
        T::send(&self.tx, msg)
    }

    /// Try to send message `M` to the actor `A`
    ///
    /// This function fails if receiver is full or closed.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where T: MessageDestination<M>, T::Actor: Handler<M>, M: ResponseType + 'static,
             <T::Actor as Actor>::Context: ToEnv<T, M>,
    {
        T::try_send(&self.tx, msg)
    }

    /// Send message to the actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `Future` object get dropped, message cancels.
    pub fn call_fut<M>(&self, msg: M) -> T::Future
        where T: MessageDestination<M>, T::Actor: Handler<M>, M: ResponseType + 'static,
             <T::Actor as Actor>::Context: ToEnv<T, M>,
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
              M: ResponseType + 'static,
              B: Actor,
             <T::Actor as Actor>::Context: ToEnv<T, M>,
    {
        T::call(&self.tx, act, msg)
    }

    /// Get `Subscriber` for specific message type
    pub fn subscriber<M>(self) -> Subscriber<M>
        where T: MessageDestination<M>, T::Actor: Handler<M>, M: ResponseType + 'static,
    {
        T::subscriber(self.tx)
    }
}

impl<T: Destination> Clone for Addr<T> {
    fn clone(&self) -> Addr<T> {
        Addr{tx: self.tx.clone()}
    }
}
