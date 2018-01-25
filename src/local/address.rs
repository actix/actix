use actor::{Actor, AsyncContext};
use address::{SendError, Subscriber};
use handler::{Handler, ResponseType};

use super::channel::LocalAddrSender;
use super::message::{LocalRequest, LocalFutRequest, UpgradeAddress};


/// Local address of the actor
///
/// Actor has to run in the same thread as owner of the address.
pub struct LocalAddress<A> where A: Actor, A::Context: AsyncContext<A> {
    tx: LocalAddrSender<A>
}

impl<A> Clone for LocalAddress<A> where A: Actor, A::Context: AsyncContext<A> {
    fn clone(&self) -> Self {
        LocalAddress{tx: self.tx.clone()}
    }
}

impl<A> LocalAddress<A> where A: Actor, A::Context: AsyncContext<A> {

    pub(crate) fn new(sender: LocalAddrSender<A>) -> LocalAddress<A> {
        LocalAddress{tx: sender}
    }

    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    /// Send message `M` to the actor `A`
    ///
    /// This method ignores receiver capacity and silently fails if actor is closed.
    pub fn send<M>(&self, msg: M) where A: Handler<M>, M: ResponseType + 'static {
        let _ = self.tx.do_send(msg);
    }

    /// Try to send message `M` to the actor `A`
    ///
    /// This function fails if receiver if full or closed.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where A: Handler<M>, M: ResponseType + 'static
    {
        self.tx.try_send(msg)
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
            Err(SendError::NotReady(msg)) =>
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
            Err(SendError::NotReady(msg)) =>
                LocalFutRequest::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                LocalFutRequest::new(None, None),
        }
    }

    /// Upgrade address to remote Address.
    pub fn upgrade(&self) -> UpgradeAddress<A> {
        UpgradeAddress::new(&self.tx)
    }

    /// Get `Subscriber` for specific message type
    pub fn subscriber<M>(&self) -> Box<Subscriber<M>>
        where A: Handler<M>, M: ResponseType + 'static
    {
        Box::new(Clone::clone(self))
    }
}

impl<A, M> Subscriber<M> for LocalAddress<A>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType + 'static
{
    fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.try_send(msg)
    }

    #[doc(hidden)]
    fn boxed(&self) -> Box<Subscriber<M>> {
        Box::new(self.clone())
    }
}
