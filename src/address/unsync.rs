use std::rc::Rc;
use std::marker::PhantomData;

use futures::unsync::oneshot::{Receiver, Sender};

use actor::{Actor, AsyncContext};
use handler::{Handler, Message, MessageResponse};
use context::Context;

use super::ToEnvelope;
use super::message::{Request, RequestFut};
use super::envelope::EnvelopeProxy;
use super::unsync_channel::{UnsyncSender, UnsyncAddrSender};
use super::UnsyncSubscriberRequest;
use super::{Destination, MessageDestination, ActorMessageDestination, SendError};


/// Unsync destination of the actor
///
/// Actor has to run in the same thread as owner of the address.
pub struct Unsync<A: Actor> {
    act: PhantomData<Rc<A>>,
    proxy: Box<EnvelopeProxy<Actor=A>>,
}

impl<A: Actor> Unsync<A> {
    pub fn new<M>(msg: M, tx: Option<Sender<M::Result>>) -> Unsync<A>
        where M: Message + 'static,
              A: Handler<M>, A::Context: AsyncContext<A>
    {
        Unsync {
            proxy: Box::new(
                InnerLocalEnvelope{msg: Some(msg),
                                   tx: tx,
                                   act: PhantomData}),
            act: PhantomData}
    }

    pub fn handle(&mut self, act: &mut A, ctx: &mut A::Context) {
        self.proxy.handle(act, ctx)
    }
}

impl<A: Actor> Destination for Unsync<A>
    where A::Context: AsyncContext<A>
{
    type Actor = A;
    type Transport = UnsyncAddrSender<A>;

    /// Indicates if actor is still alive
    fn connected(tx: &Self::Transport) -> bool {
        tx.connected()
    }
}

impl<M, A: Actor> MessageDestination<M> for Unsync<A>
    where M: Message + 'static,
          A: Handler<M>, A::Context: AsyncContext<A> + ToEnvelope<Self, M>
{
    type Future = RequestFut<Unsync<A>, M>;
    type Subscriber = Subscriber<M>;
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
            Err(SendError::Full(msg)) => RequestFut::new(None, Some((tx.clone(), msg))),
            Err(SendError::Closed(_)) => RequestFut::new(None, None),
        }
    }

    /// Get `Subscriber` for specific message type
    fn subscriber(tx: Self::Transport) -> Subscriber<M> {
        Subscriber(tx.into_sender())
    }
}

impl<A: Actor, B: Actor, M> ActorMessageDestination<M, B> for Unsync<A>
    where M: Message + 'static,
          A: Handler<M>, A::Context: AsyncContext<A> + ToEnvelope<Self, M>,
          B::Context: AsyncContext<B>,
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

struct InnerLocalEnvelope<A, M> where M: Message {
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<Sender<M::Result>>,
}

impl<A, M> EnvelopeProxy for InnerLocalEnvelope<A, M>
    where M: Message + 'static,
          A: Actor + Handler<M>, A::Context: AsyncContext<A>
{
    type Actor = A;

    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context)
    {
        let tx = self.tx.take();
        if tx.is_some() && tx.as_ref().unwrap().is_canceled() {
            return
        }
        if let Some(msg) = self.msg.take() {
            <Self::Actor as Handler<M>>::handle(act, msg, ctx).handle(ctx, tx)
        }
    }
}

impl<A, M: Message + 'static> ToEnvelope<Unsync<A>, M> for Context<A>
    where A: Actor<Context=Context<A>> + Handler<M>
{
    fn pack(msg: M, tx: Option<Sender<M::Result>>) -> Unsync<A> {
        Unsync::new(msg, tx)
    }
}

/// `Subscriber` type allows to send one specific message to an actor.
///
/// You can get subscriber with `Address::into_subscriber::<M>()` method.
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub struct Subscriber<M: Message+'static>(pub(crate) Box<UnsyncSender<M>>);

impl<M: Message + 'static> Subscriber<M> {
    /// Send message
    ///
    /// Sends message even if actor's mailbox is full
    pub fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.0.do_send(msg)
    }

    /// Try send message
    ///
    /// This method fails if actor's mailbox is full or closed. This method
    /// register current task in receivers queue.
    pub fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.0.try_send(msg)
    }

    /// Send message and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded. if returned `Receiver`
    /// object get dropped, message cancels.
    pub fn call(&self, msg: M) -> UnsyncSubscriberRequest<M> {
        match self.0.send(msg) {
            Ok(rx) => UnsyncSubscriberRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) =>
                UnsyncSubscriberRequest::new(None, Some((self.0.boxed(), msg))),
            Err(SendError::Closed(_)) =>
                UnsyncSubscriberRequest::new(None, None),
        }
    }
}

impl<M: Message + 'static> Clone for Subscriber<M> {
    fn clone(&self) -> Subscriber<M> {
        Subscriber(self.0.boxed())
    }
}
