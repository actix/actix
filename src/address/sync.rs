use futures::sync::oneshot::{Sender, Receiver};

use actor::Actor;
use handler::{Handler, MessageResult, ResponseType};

use super::envelope::{ToEnvelope, EnvelopeProxy};
use super::sync_address::SyncSubscriber;
use super::sync_channel::SyncAddressSender;
use super::{Request, RequestFut};
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
              M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
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
          M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
{
    type Future = RequestFut<Syn<A>, M>;
    type Subscriber = SyncSubscriber<M>;
    type ResultSender = Sender<MessageResult<M>>;
    type ResultReceiver = Receiver<MessageResult<M>>;

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
          M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
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
