use std::rc::Rc;
use std::marker::PhantomData;
use futures::unsync::oneshot::Sender;

use actor::{Actor, AsyncContext};
use handler::{Handler, ResponseType, MessageResponse};
use super::EnvelopeProxy;

pub struct LocalEnvelope<A>{
    pub env: Box<EnvelopeProxy<Actor=A>>,
    act: PhantomData<Rc<A>>
}

impl<A> LocalEnvelope<A> where A: Actor {

    pub(crate) fn new<M>(msg: M, tx: Option<Sender<Result<M::Item, M::Error>>>) -> Self
        where M: ResponseType + 'static,
              A: Actor + Handler<M>, A::Context: AsyncContext<A>
    {
        LocalEnvelope {
            env: Box::new(
                InnerLocalEnvelope{msg: Some(msg),
                                   tx: tx,
                                   act: PhantomData}),
            act: PhantomData}
    }
}

struct InnerLocalEnvelope<A, M> where M: ResponseType {
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<Sender<Result<M::Item, M::Error>>>,
}

impl<A, M> EnvelopeProxy for InnerLocalEnvelope<A, M>
    where M: ResponseType + 'static,
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
