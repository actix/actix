use std::{mem, fmt};
use futures::{Async, AsyncSink, Poll, Sink, StartSend};

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
pub use self::local_address::Address;
pub use self::local_message::{LocalRequest, LocalFutRequest};
pub(crate) use self::local_envelope::LocalEnvelope;
pub(crate) use self::local_channel::LocalAddrReceiver;

pub use self::sync_address::SyncAddress;
pub use self::sync_message::{Request, RequestFut};
pub(crate) use self::sync_channel::SyncAddressReceiver;


pub enum SendError<T> {
    Full(T),
    Closed(T),
}

#[derive(Fail)]
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

/// Subscriber trait describes ability of actor to receive one specific message
///
/// You can get subscriber with `Address::subscriber()` or
/// `Address::subscriber()` methods. Both methods returns boxed trait object.
///
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub trait Subscriber<M: 'static> {

    /// Send message
    fn send(&self, msg: M) -> Result<(), SendError<M>>;

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
    fn try_send(&self, msg: M) -> Result<(), SendError<M>>;

    #[doc(hidden)]
    /// Create boxed clone of the current subscriber
    fn boxed(&self) -> Box<Subscriber<M>>;
}

/// Convenience impl to allow boxed Subscriber objects to be cloned using `Clone.clone()`.
impl<M: 'static> Clone for Box<Subscriber<M>> {
    fn clone(&self) -> Box<Subscriber<M>> {
        self.boxed()
    }
}

impl<M: 'static> fmt::Debug for Box<Subscriber<M>> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Subscriber<_>")
    }
}

/// Convenience impl to allow boxed Subscriber objects to be cloned using `Clone.clone()`.
impl<M: 'static> Clone for Box<Subscriber<M> + Send> {
    fn clone(&self) -> Box<Subscriber<M> + Send> {
        // simplify ergonomics of `+Send` subscriber, otherwise
        // it would require new trait with custom `.boxed()` method.
        unsafe { mem::transmute(self.boxed()) }
    }
}

impl<M: 'static> fmt::Debug for Box<Subscriber<M> + Send> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Subscriber<_> + Send")
    }
}

impl<M: 'static> Sink for Box<Subscriber<M>> {
    type SinkItem = M;
    type SinkError = M;

    fn start_send(&mut self, msg: M) -> StartSend<M, M> {
        match self.try_send(msg) {
            Ok(()) => Ok(AsyncSink::Ready),
            Err(err) => Err(err.into_inner()),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), M> {
        Ok(Async::Ready(()))
    }

    fn close(&mut self) -> Poll<(), M> {
        Ok(Async::Ready(()))
    }
}

impl<M: 'static> Sink for Box<Subscriber<M> + Send> {
    type SinkItem = M;
    type SinkError = M;

    fn start_send(&mut self, msg: M) -> StartSend<M, M> {
        match self.try_send(msg) {
            Ok(()) => Ok(AsyncSink::Ready),
            Err(err) => Err(err.into_inner()),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), M> {
        Ok(Async::Ready(()))
    }

    fn close(&mut self) -> Poll<(), M> {
        Ok(Async::Ready(()))
    }
}
