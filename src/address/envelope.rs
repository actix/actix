use futures::sync::oneshot::Sender;
use std::marker::PhantomData;

use super::{MessageDestination, MessageDestinationTransport, Syn};
use actor::{Actor, AsyncContext};
use context::Context;
use handler::{Handler, Message, MessageResponse};

/// Converter trait, packs message to suitable envelope
pub trait ToEnvelope<T: MessageDestination<A, M>, A, M: Message + 'static>
where
    T::Transport: MessageDestinationTransport<T, A, M>,
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<T, A, M>,
{
    /// Pack message into suitable envelope
    fn pack(msg: M, tx: Option<T::ResultSender>) -> T::Envelope;
}

pub trait EnvelopeProxy {
    type Actor: Actor;

    /// handle message within new actor and context
    fn handle(
        &mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context,
    );
}

pub struct MessageEnvelope<M>
where
    M: Message + Send,
    M::Result: Send,
{
    msg: M,
}

impl<M: Message + Send> MessageEnvelope<M>
where
    M: Message + Send,
    M::Result: Send,
{
    pub fn into_inner(self) -> M {
        self.msg
    }
}

impl<M> From<M> for MessageEnvelope<M>
where
    M: Message + Send,
    M::Result: Send,
{
    fn from(msg: M) -> MessageEnvelope<M> {
        MessageEnvelope { msg }
    }
}

impl<A, M> ToEnvelope<Syn, A, M> for Context<A>
where
    A: Actor<Context = Context<A>> + Handler<M>,
    M: Message + Send + 'static,
    M::Result: Send,
{
    fn pack(msg: M, tx: Option<Sender<M::Result>>) -> Envelope<A> {
        Envelope::new(msg, tx)
    }
}

pub struct Envelope<A: Actor>(Box<EnvelopeProxy<Actor = A> + Send>);

unsafe impl<A: Actor> Send for Envelope<A> {}

impl<A: Actor> Envelope<A> {
    pub fn new<M>(msg: M, tx: Option<Sender<M::Result>>) -> Envelope<A>
    where
        A: Handler<M>,
        A::Context: AsyncContext<A>,
        M: Message + Send + 'static,
        M::Result: Send,
    {
        Envelope(Box::new(SyncEnvelopeProxy {
            tx,
            msg: Some(msg),
            act: PhantomData,
        }))
    }

    pub fn with_proxy(proxy: Box<EnvelopeProxy<Actor = A> + Send>) -> Envelope<A> {
        Envelope(proxy)
    }
}

impl<A: Actor> EnvelopeProxy for Envelope<A> {
    type Actor = A;

    fn handle(
        &mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context,
    ) {
        self.0.handle(act, ctx)
    }
}

pub struct SyncEnvelopeProxy<A, M>
where
    M: Message + Send,
{
    act: PhantomData<A>,
    msg: Option<M>,
    tx: Option<Sender<M::Result>>,
}

unsafe impl<A, M: Message + Send> Send for SyncEnvelopeProxy<A, M> {}

impl<A, M> EnvelopeProxy for SyncEnvelopeProxy<A, M>
where
    M: Message + Send + 'static,
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
{
    type Actor = A;

    fn handle(
        &mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context,
    ) {
        let tx = self.tx.take();
        if tx.is_some() && tx.as_ref().unwrap().is_canceled() {
            return;
        }

        if let Some(msg) = self.msg.take() {
            let fut = <Self::Actor as Handler<M>>::handle(act, msg, ctx);
            fut.handle(ctx, tx)
        }
    }
}
