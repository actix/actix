use futures::sync::oneshot::{Sender, Receiver};

use actor::Actor;
use handler::{Handler, Message};

use super::envelope::{ToEnvelope, EnvelopeProxy};
use super::sync_channel::{SyncSender, SyncAddressSender};
use super::{Request, RequestFut, SyncSubscriberRequest};
use super::{Destination, MessageDestination, ActorMessageDestination,
            DestinationSender, SendError};


/// Sync destination of the actor. Actor can run in different thread
pub struct Syn<A: Actor> {
    proxy: Box<EnvelopeProxy<Actor=A> + Send>,
}

unsafe impl<A: Actor> Send for Syn<A> {}

impl<A: Actor> Syn<A> {
    pub fn new<M>(proxy: Box<EnvelopeProxy<Actor=A> + Send>) -> Syn<A>
        where A: Handler<M>,
              M: Message + Send + 'static, M::Result: Send,
    {
        Syn{proxy: proxy}
    }

    pub fn handle(&mut self, act: &mut A, ctx: &mut A::Context) {
        self.proxy.handle(act, ctx)
    }
}

impl<A: Actor> Destination for Syn<A>
{
    type Actor = A;
    type Transport = SyncAddressSender<A>;

    /// Indicates if actor is still alive
    fn connected(tx: &Self::Transport) -> bool {
        tx.connected()
    }
}

impl<M, A: Actor> MessageDestination<M> for Syn<A>
    where A: Handler<M>, A::Context: ToEnvelope<Self, M>,
          M: Message + Send + 'static, M::Result: Send,
{
    type Future = RequestFut<Syn<A>, M>;
    type Subscriber = SyncSubscriber<M>;
    type ResultSender = Sender<M::Result>;
    type ResultReceiver = Receiver<M::Result>;

    fn send(tx: &Self::Transport, msg: M) {
        let _ = tx.do_send(msg);
    }

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.try_send(msg, false)
    }

    fn call_fut(tx: &Self::Transport, msg: M) -> Self::Future {
        match tx.send(msg) {
            Ok(rx) => RequestFut::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                RequestFut::new(None, Some((tx.clone(), msg))),
            Err(SendError::Closed(_)) =>
                RequestFut::new(None, None),
        }
    }

    /// Get `Subscriber` for specific message type
    fn subscriber(tx: Self::Transport) -> SyncSubscriber<M> {
        SyncSubscriber{tx: tx.into_sender()}
    }
}

impl<A: Actor, B: Actor, M> ActorMessageDestination<M, B> for Syn<A>
    where A: Handler<M>, A::Context: ToEnvelope<Self, M>,
          Self::Transport: DestinationSender<Self, M>,
          M: Message + Send + 'static, M::Result: Send,
{
    type ActFuture = Request<Self, B, M>;

    fn call(tx: &Self::Transport, _: &B, msg: M) -> Self::ActFuture {
        match tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::Full(msg)) => Request::new(None, Some((tx.clone(), msg))),
            Err(SendError::Closed(_)) => Request::new(None, None)
        }
    }
}

/// `SyncSubscriber` type allows to send one specific message to an actor.
///
/// You can get subscriber with `SyncAddress::into_subscriber::<M>()` method.
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub struct SyncSubscriber<M>
    where M: Message + Send + 'static, M::Result: Send,
{
    pub(crate) tx: Box<SyncSender<M>>
}

unsafe impl<M> Send for SyncSubscriber<M>
    where M: Message + Send + 'static, M::Result: Send {}

impl<M> SyncSubscriber<M>
    where M: Message + Send + 'static, M::Result: Send,
{

    /// Send message
    ///
    /// Sends message even if actor's mailbox is full
    pub fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.do_send(msg)
    }

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
    pub fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.tx.try_send(msg)
    }

    /// Send message and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Receiver`
    /// object get dropped, message cancels.
    pub fn call(&self, msg: M) -> SyncSubscriberRequest<M> {
        match self.tx.send(msg) {
            Ok(rx) => SyncSubscriberRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                SyncSubscriberRequest::new(None, Some((self.tx.boxed(), msg))),
            Err(SendError::Closed(_)) =>
                SyncSubscriberRequest::new(None, None),
        }
    }
}

impl<M> Clone for SyncSubscriber<M>
    where M: Message + Send + 'static, M::Result: Send,
{
    fn clone(&self) -> SyncSubscriber<M> {
        SyncSubscriber{tx: self.tx.boxed()}
    }
}
