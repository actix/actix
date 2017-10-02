use std::marker::PhantomData;
use futures::{Async, Poll};
use futures::unsync::oneshot::Sender;
use futures::sync::oneshot::Sender as SyncSender;

use fut::ActorFuture;
use actor::{Actor, AsyncContext, Handler};
use message::Response;


pub(crate) struct Envelope<A>(Box<EnvelopeProxy<Actor=A>>);

impl<A> Envelope<A> where A: Actor {

    pub fn local<M>(msg: M, tx: Option<Sender<Result<A::Item, A::Error>>>) -> Self
        where M: 'static,
              A: Actor + Handler<M>,
              A::Context: AsyncContext<A>
    {
        Envelope(Box::new(LocalEnvelope{msg: Some(msg), tx: tx, act: PhantomData}))
    }

    pub fn remote<M>(msg: M, tx: Option<SyncSender<Result<A::Item, A::Error>>>) -> Self
        where M: Send + 'static,
              A: Actor + Handler<M>,
              A::Item: Send,
              A::Error: Send,
              A::Context: AsyncContext<A>,
    {
        Envelope(Box::new(RemoteEnvelope{msg: Some(msg), tx: tx, act: PhantomData}))
    }

    pub fn handle(&mut self, act: &mut A, ctx: &mut A::Context) {
        self.0.handle(act, ctx)
    }
}

// This is not safe! Local envelope could be send to different thread!
unsafe impl<T> Send for Envelope<T> {}


trait EnvelopeProxy {

    type Actor: Actor;

    /// handle message within new actor and context
    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context);
}

struct LocalEnvelope<A, M> where A: Actor + Handler<M>, A::Context: AsyncContext<A> {
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<Sender<Result<A::Item, A::Error>>>,
}

impl<A, M> EnvelopeProxy for LocalEnvelope<A, M>
    where M: 'static,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>
{
    type Actor = A;

    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context)
    {
        if let Some(msg) = self.msg.take() {
            let fut = <Self::Actor as Handler<M>>::handle(act, msg, ctx);
            let tx = if let Some(tx) = self.tx.take() {
                Some(EnvelopFutureItem::Local(tx))
            } else {
                None
            };
            let f: EnvelopFuture<Self::Actor, _> = EnvelopFuture {
                msg: PhantomData, fut: fut, tx: tx};
            ctx.spawn(f);
        }
    }
}

struct RemoteEnvelope<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
{
    msg: Option<M>,
    act: PhantomData<A>,
    tx: Option<SyncSender<Result<A::Item, A::Error>>>,
}

impl<A, M> EnvelopeProxy for RemoteEnvelope<A, M>
    where M: 'static,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
{
    type Actor = A;

    fn handle(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context)
    {
        if let Some(msg) = self.msg.take() {
            let fut = <Self::Actor as Handler<M>>::handle(act, msg, ctx);
            let tx = if let Some(tx) = self.tx.take() {
                Some(EnvelopFutureItem::Remote(tx))
            } else {
                None
            };
            let f: EnvelopFuture<Self::Actor, _> = EnvelopFuture {
                msg: PhantomData, fut: fut, tx: tx};
            ctx.spawn(f);
        }
    }
}


enum EnvelopFutureItem<A, M> where A: Handler<M> {
    Local(Sender<Result<A::Item, A::Error>>),
    Remote(SyncSender<Result<A::Item, A::Error>>),
}

pub(crate) struct EnvelopFuture<A, M> where A: Handler<M>
{
    msg: PhantomData<M>,
    fut: Response<A, M>,
    tx: Option<EnvelopFutureItem<A, M>>,
}

impl<A, M> ActorFuture for EnvelopFuture<A, M> where A: Actor + Handler<M>
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self,
            act: &mut A,
            ctx: &mut <Self::Actor as Actor>::Context) -> Poll<Self::Item, Self::Error>
    {
        match self.fut.poll(act, ctx) {
            Ok(Async::Ready(val)) => {
                match self.tx.take() {
                    Some(EnvelopFutureItem::Local(tx)) => { let _ = tx.send(Ok(val)); },
                    Some(EnvelopFutureItem::Remote(tx)) => { let _ = tx.send(Ok(val)); },
                    _ => (),
                }
                Ok(Async::Ready(()))
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                match self.tx.take() {
                    Some(EnvelopFutureItem::Local(tx)) => { let _ = tx.send(Err(err)); },
                    Some(EnvelopFutureItem::Remote(tx)) => { let _ = tx.send(Err(err)); },
                    _ => (),
                }
                Err(())
            }
        }
    }
}
