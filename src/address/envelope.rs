use futures::sync::oneshot::Sender;
use std::marker::PhantomData;

// use super::{MessageDestination, MessageDestinationTransport, Syn};
use crate::actor::{Actor, AsyncContext};
use crate::context::Context;
use crate::handler::{Handler, Message, MessageResponse};

/// Converter trait, packs message into a suitable envelope.
pub trait ToEnvelope<A, M: Message>
where
    A: Actor + Handler<M>,
    A::Context: ToEnvelope<A, M>,
{
    /// Pack message into suitable envelope
    fn pack(msg: M, tx: Option<Sender<M::Result>>) -> Envelope<A>;
}

pub trait EnvelopeProxy {
    type Actor: Actor;

    /// handle message within new actor and context
    fn handle(
        &mut self,
        act: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
    );
}

impl<A, M> ToEnvelope<A, M> for Context<A>
where
    A: Actor<Context = Context<A>> + Handler<M>,
    M: Message + Send + 'static,
    M::Result: Send,
{
    fn pack(msg: M, tx: Option<Sender<M::Result>>) -> Envelope<A> {
        Envelope::new(msg, tx)
    }
}

pub struct Envelope<A: Actor>(Box<dyn EnvelopeProxy<Actor = A> + Send>);

impl<A: Actor> Envelope<A> {
    pub fn new<M>(msg: M, tx: Option<Sender<M::Result>>) -> Self
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

    pub fn with_proxy(proxy: Box<dyn EnvelopeProxy<Actor = A> + Send>) -> Self {
        Envelope(proxy)
    }
}

impl<A: Actor> EnvelopeProxy for Envelope<A> {
    type Actor = A;

    fn handle(
        &mut self,
        act: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
    ) {
        self.0.handle(act, ctx)
    }
}

pub struct SyncEnvelopeProxy<A, M>
where
    M: Message + Send,
    M::Result: Send,
{
    act: PhantomData<A>,
    msg: Option<M>,
    tx: Option<Sender<M::Result>>,
}

unsafe impl<A, M> Send for SyncEnvelopeProxy<A, M>
where
    M: Message + Send,
    M::Result: Send,
{
}

impl<A, M> EnvelopeProxy for SyncEnvelopeProxy<A, M>
where
    M: Message + Send + 'static,
    M::Result: Send,
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
{
    type Actor = A;

    fn handle(
        &mut self,
        act: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
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
