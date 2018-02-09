use std::fmt;

mod envelope;
mod queue;
pub(crate) mod sync_channel;
mod sync_address;
mod sync_message;
mod local_address;
mod local_channel;
mod local_message;
mod local_envelope;

use actor::{Actor, AsyncContext};

pub use self::envelope::{Envelope, EnvelopeProxy, ToEnvelope, RemoteEnvelope};
pub use self::local_address::{Address, Subscriber};
pub use self::local_message::{LocalRequest, LocalFutRequest, LocalSubscriberRequest};
pub(crate) use self::local_envelope::LocalEnvelope;
pub(crate) use self::local_channel::LocalAddrReceiver;

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

impl<A> ActorAddress<A, Address<A>> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> Address<A> {
        ctx.local_address()
    }
}

impl<A> ActorAddress<A, SyncAddress<A>> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> SyncAddress<A> {
        ctx.sync_address()
    }
}

impl<A> ActorAddress<A, (Address<A>, SyncAddress<A>)> for A
    where A: Actor, A::Context: AsyncContext<A>
{
    fn get(ctx: &mut A::Context) -> (Address<A>, SyncAddress<A>) {
        (ctx.local_address(), ctx.sync_address())
    }
}

impl<A> ActorAddress<A, ()> for A where A: Actor {
    fn get(_: &mut A::Context) -> () {
        ()
    }
}
