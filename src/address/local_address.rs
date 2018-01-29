use actor::{Actor, AsyncContext};
use address::{SendError, Subscriber};
use handler::{Handler, ResponseType};

use super::local_channel::LocalAddrSender;
use super::local_message::{LocalRequest, LocalFutRequest};


/// Local address of the actor
///
/// Actor has to run in the same thread as owner of the address.
pub struct Address<A> where A: Actor, A::Context: AsyncContext<A> {
    tx: LocalAddrSender<A>
}

impl<A> Clone for Address<A> where A: Actor, A::Context: AsyncContext<A> {
    fn clone(&self) -> Self {
        Address{tx: self.tx.clone()}
    }
}

impl<A> Address<A> where A: Actor, A::Context: AsyncContext<A> {

    pub(crate) fn new(sender: LocalAddrSender<A>) -> Address<A> {
        Address{tx: sender}
    }

    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    /// Send message `M` to the actor `A`
    ///
    /// This method ignores receiver capacity, it silently fails if mailbox is closed.
    pub fn send<M>(&self, msg: M) where A: Handler<M>, M: ResponseType + 'static {
        let _ = self.tx.do_send(msg);
    }

    /// Try to send message `M` to the actor `A`
    ///
    /// This function fails if receiver if full or closed.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where A: Handler<M>, M: ResponseType + 'static
    {
        self.tx.try_send(msg, false)
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `LocalRequest` object get dropped, message cancels.
    pub fn call<B, M>(&self, _: &B, msg: M) -> LocalRequest<A, B, M>
        where A: Handler<M>, M: ResponseType + 'static, B: Actor, B::Context: AsyncContext<B>
    {
        match self.tx.send(msg) {
            Ok(rx) => LocalRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                LocalRequest::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                LocalRequest::new(None, None)
        }
    }

    /// Send message to the actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `LocalReceiver` object get dropped, message cancels.
    pub fn call_fut<M>(&self, msg: M) -> LocalFutRequest<A, M>
        where A: Handler<M>, M: ResponseType + 'static
    {
        match self.tx.send(msg) {
            Ok(rx) => LocalFutRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                LocalFutRequest::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                LocalFutRequest::new(None, None),
        }
    }

    /// Get `Subscriber` for specific message type
    pub fn into_subscriber<M>(self) -> Box<Subscriber<M>>
        where A: Handler<M>, M: ResponseType + 'static
    {
        Box::new(self)
    }
}

impl<A, M> Subscriber<M> for Address<A>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType + 'static
{
    fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.do_send(msg)
    }

    fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.try_send(msg, true)
    }

    #[doc(hidden)]
    fn boxed(&self) -> Box<Subscriber<M>> {
        Box::new(self.clone())
    }
}
