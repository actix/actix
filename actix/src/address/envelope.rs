use tokio::sync::oneshot::Sender;

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

pub trait EnvelopeProxy<A: Actor> {
    /// handle message within new actor and context
    fn handle(&mut self, act: &mut A, ctx: &mut A::Context);
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

pub struct Envelope<A: Actor>(Box<dyn EnvelopeProxy<A> + Send>);

impl<A: Actor> Envelope<A> {
    pub fn new<M>(msg: M, tx: Option<Sender<M::Result>>) -> Self
    where
        A: Handler<M>,
        A::Context: AsyncContext<A>,
        M: Message + Send + 'static,
        M::Result: Send,
    {
        Envelope(Box::new(SyncEnvelopeProxy { tx, msg: Some(msg) }))
    }

    pub fn with_proxy(proxy: Box<dyn EnvelopeProxy<A> + Send>) -> Self {
        Envelope(proxy)
    }
}

impl<A: Actor> EnvelopeProxy<A> for Envelope<A> {
    fn handle(&mut self, act: &mut A, ctx: &mut <A as Actor>::Context) {
        self.0.handle(act, ctx)
    }
}

pub struct SyncEnvelopeProxy<M>
where
    M: Message + Send,
    M::Result: Send,
{
    msg: Option<M>,
    tx: Option<Sender<M::Result>>,
}

impl<A, M> EnvelopeProxy<A> for SyncEnvelopeProxy<M>
where
    M: Message + Send + 'static,
    M::Result: Send,
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
{
    fn handle(&mut self, act: &mut A, ctx: &mut <A as Actor>::Context) {
        let tx = self.tx.take();
        if tx.is_some() && tx.as_ref().unwrap().is_closed() {
            return;
        }

        if let Some(msg) = self.msg.take() {
            let fut = <A as Handler<M>>::handle(act, msg, ctx);
            fut.handle(ctx, tx)
        }
    }
}
