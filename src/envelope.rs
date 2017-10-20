use std::marker::PhantomData;
use futures::{Async, Poll};
use futures::unsync::oneshot::Sender;
use futures::sync::oneshot::Sender as SyncSender;

use fut::ActorFuture;
use actor::{Actor, AsyncContext, Handler, ResponseType};
use message::Response;
use context::Context;


/// Converter trait, packs message to suitable envelope
pub trait ToEnvelope<A, M>
    where A: Actor + Handler<M>,
          M: 'static,
{
    /// Pack message into envelope
    fn pack(msg: M, tx: Option<SyncSender<Result<A::Item, A::Error>>>) -> Envelope<A>;
}

impl<A, M> ToEnvelope<A, M> for Context<A>
    where A: Actor<Context=Context<A>> + Handler<M>,
          M: Send + 'static,
          A::Item: Send,
          A::Error: Send
{
    fn pack(msg: M, tx: Option<SyncSender<Result<A::Item, A::Error>>>) -> Envelope<A>
    {
        Envelope(Box::new(RemoteEnvelope{msg: Some(msg), tx: tx}))
    }
}

pub struct Envelope<A>(Box<EnvelopeProxy<Actor=A>>);

impl<A> Envelope<A> where A: Actor {

    pub(crate) fn new<T>(envelop: T) -> Self
        where T: EnvelopeProxy<Actor=A> + Sized + 'static
    {
        Envelope(Box::new(envelop))
    }

    pub(crate) fn local<M>(msg: M, tx: Option<Sender<Result<A::Item, A::Error>>>) -> Self
        where M: 'static,
              A: Actor + Handler<M>,
              A::Context: AsyncContext<A>
    {
        Envelope(Box::new(LocalEnvelope{msg: Some(msg), tx: tx, act: PhantomData}))
    }

    pub(crate) fn handle(&mut self, act: &mut A, ctx: &mut A::Context) {
        self.0.handle(act, ctx)
    }
}

// This is not safe! Local envelope could be send to different thread!
unsafe impl<T> Send for Envelope<T> {}

pub trait EnvelopeProxy {

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

pub struct RemoteEnvelope<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
{
    msg: Option<M>,
    tx: Option<SyncSender<Result<A::Item, A::Error>>>,
}

impl<A, M> RemoteEnvelope<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
{
    pub fn new(msg: M, tx: Option<SyncSender<Result<A::Item, A::Error>>>) -> RemoteEnvelope<A, M>
        where M: Send + 'static,
              A: Handler<M>, <A as ResponseType<M>>::Item: Send, <A as ResponseType<M>>::Item: Send
    {
        RemoteEnvelope{msg: Some(msg), tx: tx}
    }
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

impl<A, M> From<RemoteEnvelope<A, M>> for Envelope<A>
    where M: Send + 'static,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
{
    fn from(env: RemoteEnvelope<A, M>) -> Self {
        Envelope::new(env)
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
