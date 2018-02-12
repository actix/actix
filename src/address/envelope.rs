use std::marker::PhantomData;
use futures::sync::oneshot::Sender;

use actor::{Actor, AsyncContext};
use context::Context;
use handler::{Handler, Message, MessageResponse};
use super::{Syn, MessageDestination};
use super::DestinationSender;


/// Converter trait, packs message to suitable envelope
pub trait ToEnvelope<T: MessageDestination<M>, M: Message + 'static>
    where T::Actor: Actor + Handler<M>,
          T::Transport: DestinationSender<T, M>,
         <T::Actor as Actor>::Context: ToEnvelope<T, M>,
{
    /// Pack message into suitable envelope
    fn pack(msg: M, tx: Option<T::ResultSender>) -> T;
}

impl<A, M> ToEnvelope<Syn<A>, M> for Context<A>
    where A: Actor<Context=Context<A>> + Handler<M>,
          M: Message + Send + 'static, M::Result: Send,
{
    fn pack(msg: M, tx: Option<Sender<M::Result>>) -> Syn<A> {
        Syn::new(Box::new(
            RemoteEnvelope{msg: Some(msg),
                           tx: tx,
                           act: PhantomData}))
    }
}

pub trait EnvelopeProxy {

    type Actor: Actor;

    /// handle message within new actor and context
    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context);
}

pub struct RemoteEnvelope<A, M> where M: Message {
    act: PhantomData<A>,
    msg: Option<M>,
    tx: Option<Sender<M::Result>>,
}

unsafe impl<A, M: Message> Send for RemoteEnvelope<A, M> {}

impl<A, M> RemoteEnvelope<A, M> where A: Actor, M: Message {

    pub fn envelope(msg: M, tx: Option<Sender<M::Result>>) -> RemoteEnvelope<A, M>
        where A: Handler<M>,
              M: Send + 'static, M::Result: Send
    {
        RemoteEnvelope{msg: Some(msg),
                       tx: tx,
                       act: PhantomData}
    }
}

impl<A, M> EnvelopeProxy for RemoteEnvelope<A, M>
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
