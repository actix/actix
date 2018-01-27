use actor::Actor;
use handler::{Handler, ResponseType};

use super::{Request, RequestFut, SendError, Subscriber, ToEnvelope};
use super::sync_channel::AddressSender;

/// `Send` address of the actor. Actor can run in different thread
pub struct Address<A> where A: Actor {
    tx: AddressSender<A>
}

unsafe impl<A> Send for Address<A> where A: Actor {}
unsafe impl<A> Sync for Address<A> where A: Actor {}

impl<A> Clone for Address<A> where A: Actor {
    fn clone(&self) -> Self {
        Address{tx: self.tx.clone()}
    }
}

impl<A> Address<A> where A: Actor {

    pub(crate) fn new(sender: AddressSender<A>) -> Address<A> {
        Address{tx: sender}
    }

    /// Indicates if actor is still alive
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    /// Send message `M` to actor `A`
    ///
    /// This function ignores receiver capacity and always successed.
    /// Message cold be sent to an actor running in different thread.
    pub fn send<M>(&self, msg: M)
        where A: Handler<M>, <A as Actor>::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
    {
        let _ = self.tx.do_send(msg);
    }

    /// Send message `M` to actor `A`
    ///
    /// This function fails if receiver if full or closed.
    /// Message cold be sent to actor running in different thread.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where A: Handler<M>, <A as Actor>::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
    {
        self.tx.try_send(msg)
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// if returned `Request` object get dropped, message cancels.
    pub fn call<B: Actor, M>(&self, _: &B, msg: M) -> Request<A, B, M>
        where A: Handler<M>, A::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
    {
        match self.tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::NotReady(msg)) =>
                Request::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                Request::new(None, None),
        }
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// if returned `Receiver` object get dropped, message cancels.
    pub fn call_fut<M>(&self, msg: M) -> RequestFut<A, M>
        where A: Handler<M>, A::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
    {
        match self.tx.send(msg) {
            Ok(rx) => RequestFut::new(Some(rx), None),
            Err(SendError::NotReady(msg)) =>
                RequestFut::new(None, Some((self.tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                RequestFut::new(None, None),
        }
    }

    /// Convert address to a `Subscriber` for specific message type
    pub fn into_subscriber<M: 'static + Send>(self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>, A::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send {
        Box::new(self)
    }
}

impl<A, M> Subscriber<M> for Address<A>
    where A: Actor + Handler<M>,
          <A as Actor>::Context: ToEnvelope<A>,
          M: ResponseType + Send + 'static,
          M::Item: Send, M::Error: Send,
{
    fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.try_send(msg)
    }

    #[doc(hidden)]
    fn boxed(&self) -> Box<Subscriber<M>> {
        Box::new(self.clone())
    }
}
