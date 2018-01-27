use std::rc::Rc;
use std::marker::PhantomData;
use futures::{Async, Poll};
use futures::unsync::oneshot::Sender;

use fut::ActorFuture;
use actor::{Actor, AsyncContext};
use handler::{Handler, Response, ResponseType, IntoResponse};
use super::EnvelopeProxy;

pub struct LocalEnvelope<A>{
    pub env: Box<EnvelopeProxy<Actor=A>>,
    act: PhantomData<Rc<A>>
}

impl<A> LocalEnvelope<A> where A: Actor {

    pub(crate) fn new<M>(msg: M, tx: Option<Sender<Result<M::Item, M::Error>>>,
                         cancel_on_drop: bool) -> Self
        where M: ResponseType + 'static,
              A: Actor + Handler<M>, A::Context: AsyncContext<A>
    {
        LocalEnvelope {
            env: Box::new(
                InnerLocalEnvelope{msg: Some(msg),
                                   tx: tx,
                                   act: PhantomData,
                                   cancel_on_drop: cancel_on_drop}),
            act: PhantomData}
    }
}

struct InnerLocalEnvelope<A, M> where M: ResponseType {
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<Sender<Result<M::Item, M::Error>>>,
    cancel_on_drop: bool,
}

impl<A, M> EnvelopeProxy for InnerLocalEnvelope<A, M>
    where M: ResponseType + 'static,
          A: Actor + Handler<M>, A::Context: AsyncContext<A>
{
    type Actor = A;

    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context)
    {
        let tx = self.tx.take();
        if tx.is_some() && self.cancel_on_drop && tx.as_ref().unwrap().is_canceled() {
            return
        }
        if let Some(msg) = self.msg.take() {
            let fut = <Self::Actor as Handler<M>>::handle(act, msg, ctx);
            let f: EnvelopFuture<Self::Actor, _> = EnvelopFuture {
                msg: PhantomData, fut: fut.into_response(), tx: tx};
            ctx.spawn(f);
        }
    }
}

struct EnvelopFuture<A, M> where A: Actor, M: ResponseType {
    msg: PhantomData<M>,
    fut: Response<A, M>,
    tx: Option<Sender<Result<M::Item, M::Error>>>
}

impl<A, M> ActorFuture for EnvelopFuture<A, M>
    where A: Actor + Handler<M>,
          M: ResponseType,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self,
            act: &mut A,
            ctx: &mut <Self::Actor as Actor>::Context) -> Poll<Self::Item, Self::Error>
    {
        match self.fut.poll_response(act, ctx) {
            Ok(Async::Ready(val)) => {
                if let Some(tx) = self.tx.take() {
                    let _ = tx.send(Ok(val));
                }
                Ok(Async::Ready(()))
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                if let Some(tx) = self.tx.take() {
                    let _ = tx.send(Err(err));
                }
                Err(())
            }
        }
    }
}
