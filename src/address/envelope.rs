use std::marker::PhantomData;
use futures::sync::oneshot::Sender as SyncSender;
use futures::unsync::oneshot::Sender as UnsyncSender;

use actor::{Actor, AsyncContext};
use context::Context;
use handler::{Handler, Message, MessageResponse};
use super::{DestinationSender, MessageDestination, Syn, Unsync};


/// Converter trait, packs message to suitable envelope
pub trait ToEnvelope<T: MessageDestination<M>, M: Message + 'static>
    where T::Actor: Actor + Handler<M>,
          T::Transport: DestinationSender<T, M>,
         <T::Actor as Actor>::Context: ToEnvelope<T, M>,
{
    /// Pack message into suitable envelope
    fn pack(msg: M, tx: Option<T::ResultSender>) -> T;
}

pub trait EnvelopeProxy {

    type Actor: Actor;

    /// handle message within new actor and context
    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context);
}

impl<A, M> ToEnvelope<Syn<A>, M> for Context<A>
    where A: Actor<Context=Context<A>> + Handler<M>,
          M: Message + Send + 'static, M::Result: Send,
{
    fn pack(msg: M, tx: Option<SyncSender<M::Result>>) -> Syn<A> {
        Syn::new(Box::new(
            SyncEnvelope{msg: Some(msg),
                         tx: tx,
                         act: PhantomData}))
    }
}

impl<T, A, M: Message + 'static> ToEnvelope<Unsync<A>, M> for T
    where A: Actor<Context=T> + Handler<M>, T: AsyncContext<A>
{
    fn pack(msg: M, tx: Option<UnsyncSender<M::Result>>) -> Unsync<A> {
        Unsync::new(
            Box::new(
                UnsyncEnvelope{msg: Some(msg),
                               tx: tx,
                               act: PhantomData}))
    }
}

pub struct SyncEnvelope<A, M> where M: Message {
    act: PhantomData<A>,
    msg: Option<M>,
    tx: Option<SyncSender<M::Result>>,
}

unsafe impl<A, M: Message> Send for SyncEnvelope<A, M> {}

impl<A, M> SyncEnvelope<A, M> where A: Actor, M: Message {

    pub fn envelope(msg: M, tx: Option<SyncSender<M::Result>>) -> SyncEnvelope<A, M>
        where A: Handler<M>,
              M: Send + 'static, M::Result: Send
    {
        SyncEnvelope{msg: Some(msg),
                     tx: tx,
                     act: PhantomData}
    }
}

impl<A, M> EnvelopeProxy for SyncEnvelope<A, M>
    where M: Message + 'static,
          A: Actor + Handler<M>, A::Context: AsyncContext<A>
{
    type Actor = A;

    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context) {
        let tx = self.tx.take();
        if tx.is_some() && tx.as_ref().unwrap().is_canceled() {
            return
        }

        if let Some(msg) = self.msg.take() {
            let fut = <Self::Actor as Handler<M>>::handle(act, msg, ctx);
            fut.handle(ctx, tx)
        }
    }
}

struct UnsyncEnvelope<A, M> where M: Message {
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<UnsyncSender<M::Result>>,
}

impl<A, M> EnvelopeProxy for UnsyncEnvelope<A, M>
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
