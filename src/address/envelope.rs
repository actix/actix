use futures::sync::oneshot::Sender as SyncSender;
use futures::unsync::oneshot::Sender as UnsyncSender;
use std::marker::PhantomData;

use super::{MessageDestination, MessageDestinationTransport, Syn, Unsync};
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

pub struct MessageEnvelope<M: Message> {
    msg: M,
}

impl<M: Message> MessageEnvelope<M> {
    pub fn into_inner(self) -> M {
        self.msg
    }
}

impl<M: Message> From<M> for MessageEnvelope<M> {
    fn from(msg: M) -> MessageEnvelope<M> {
        MessageEnvelope { msg }
    }
}

pub struct SyncMessageEnvelope<M: Message + Send>
where
    M::Result: Send,
{
    msg: M,
}

impl<M: Message + Send> SyncMessageEnvelope<M>
where
    M::Result: Send,
{
    pub fn into_inner(self) -> M {
        self.msg
    }
}

impl<M: Message + Send> From<M> for SyncMessageEnvelope<M>
where
    M::Result: Send,
{
    fn from(msg: M) -> SyncMessageEnvelope<M> {
        SyncMessageEnvelope { msg }
    }
}

impl<A, M> ToEnvelope<Syn, A, M> for Context<A>
where
    A: Actor<Context = Context<A>> + Handler<M>,
    M: Message + Send + 'static,
    M::Result: Send,
{
    fn pack(msg: M, tx: Option<SyncSender<M::Result>>) -> SyncEnvelope<A> {
        SyncEnvelope::new(msg, tx)
    }
}

impl<T, A, M> ToEnvelope<Unsync, A, M> for T
where
    A: Actor<Context = T> + Handler<M>,
    M: Message + 'static,
    T: AsyncContext<A>,
{
    fn pack(msg: M, tx: Option<UnsyncSender<M::Result>>) -> UnsyncEnvelope<A> {
        UnsyncEnvelope::new(msg, tx)
    }
}

pub struct SyncEnvelope<A: Actor>(Box<EnvelopeProxy<Actor = A> + Send>);

unsafe impl<A: Actor> Send for SyncEnvelope<A> {}

impl<A: Actor> SyncEnvelope<A> {
    pub fn new<M>(msg: M, tx: Option<SyncSender<M::Result>>) -> SyncEnvelope<A>
    where
        A: Handler<M>,
        A::Context: AsyncContext<A>,
        M: Message + Send + 'static,
        M::Result: Send,
    {
        SyncEnvelope(Box::new(SyncEnvelopeProxy {
            tx,
            msg: Some(msg),
            act: PhantomData,
        }))
    }

    pub fn with_proxy(proxy: Box<EnvelopeProxy<Actor = A> + Send>) -> SyncEnvelope<A> {
        SyncEnvelope(proxy)
    }
}

impl<A: Actor> EnvelopeProxy for SyncEnvelope<A> {
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
    tx: Option<SyncSender<M::Result>>,
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

pub struct UnsyncEnvelope<A: Actor>(Box<EnvelopeProxy<Actor = A>>);

impl<A: Actor> UnsyncEnvelope<A> {
    pub fn new<M>(msg: M, tx: Option<UnsyncSender<M::Result>>) -> UnsyncEnvelope<A>
    where
        A: Handler<M>,
        A::Context: AsyncContext<A>,
        M: Message + 'static,
    {
        UnsyncEnvelope(Box::new(UnsyncEnvelopeProxy {
            tx,
            msg: Some(msg),
            act: PhantomData,
        }))
    }
}

impl<A: Actor> EnvelopeProxy for UnsyncEnvelope<A> {
    type Actor = A;

    #[inline]
    fn handle(
        &mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context,
    ) {
        self.0.handle(act, ctx)
    }
}

struct UnsyncEnvelopeProxy<A, M>
where
    M: Message,
{
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<UnsyncSender<M::Result>>,
}

impl<A, M> EnvelopeProxy for UnsyncEnvelopeProxy<A, M>
where
    M: Message + 'static,
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
            <Self::Actor as Handler<M>>::handle(act, msg, ctx).handle(ctx, tx)
        }
    }
}
