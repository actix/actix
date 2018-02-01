use std::marker::PhantomData;
use std::time::Duration;
use futures::{Async, Future, Poll, Stream};
use tokio_core::reactor::Timeout;

use fut::ActorFuture;
use arbiter::Arbiter;
use actor::{Actor, ActorContext, AsyncContext};
use handler::{Handler, MessageResponse, ResponseType};


pub(crate) struct ActorWaitItem<A: Actor>(Box<ActorFuture<Item=(), Error=(), Actor=A>>);

impl<A> ActorWaitItem<A> where A: Actor, A::Context: ActorContext + AsyncContext<A> {

    #[inline]
    pub fn new<F>(fut: F) -> Self where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static {
        ActorWaitItem(Box::new(fut))
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Async<()> {
        match self.0.poll(act, ctx) {
            Ok(Async::NotReady) => {
                if ctx.state().alive() {
                    Async::NotReady
                } else {
                    Async::Ready(())
                }
            },
            Ok(Async::Ready(_)) | Err(_) => Async::Ready(()),
        }
    }
}


pub(crate)
struct ActorFutureItem<A, M, F, E> where M: ResponseType, F: Future<Item=M, Error=E> {
    fut: F,
    act: PhantomData<A>,
}

impl<A, M, F, E> ActorFutureItem<A, M, F, E>
    where M: ResponseType + 'static,
          F: Future<Item=M, Error=E>
{
    pub fn new(fut: F) -> Self {
        ActorFutureItem{fut: fut, act: PhantomData}
    }
}

impl<A, M, F, E: 'static> ActorFuture for ActorFutureItem<A, M, F, E>
    where A: Actor + Handler<Result<M, E>>, A::Context: AsyncContext<A>,
          M: ResponseType + 'static,
          F: Future<Item=M, Error=E>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        match self.fut.poll() {
            Ok(Async::Ready(msg)) => {
                let fut = <A as Handler<Result<M, E>>>::handle(act, Ok(msg), ctx);
                fut.handle::<()>(ctx, None);
                Ok(Async::Ready(()))
            },
            Err(err) => {
                let fut = <A as Handler<Result<M, E>>>::handle(act, Err(err), ctx);
                fut.handle::<()>(ctx, None);
                Ok(Async::Ready(()))
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
        }
    }
}

pub(crate)
struct ActorDelayedMessageItem<A, M> where A: Actor, M: ResponseType {
    msg: Option<M>,
    timeout: Timeout,
    act: PhantomData<A>,
    m: PhantomData<M>,
}

impl<A, M> ActorDelayedMessageItem<A, M> where A: Actor, M: ResponseType {
    pub fn new(msg: M, timeout: Duration) -> Self {
        ActorDelayedMessageItem {
            msg: Some(msg),
            timeout: Timeout::new(timeout, Arbiter::handle()).unwrap(),
            act: PhantomData,
            m: PhantomData,
        }
    }
}

impl<A, M> ActorFuture for ActorDelayedMessageItem<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        match self.timeout.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => {
                let fut = A::handle(act, self.msg.take().unwrap(), ctx);
                fut.handle::<()>(ctx, None);
                Ok(Async::Ready(()))
            },
            Err(_) => unreachable!(),
        }
    }
}

pub(crate)
struct ActorMessageItem<A, M> where A: Actor, M: ResponseType {
    msg: Option<M>,
    act: PhantomData<A>,
}

impl<A, M> ActorMessageItem<A, M> where A: Actor, M: ResponseType {
    pub fn new(msg: M) -> Self {
        ActorMessageItem{msg: Some(msg), act: PhantomData}
    }
}

impl<A, M: 'static> ActorFuture for ActorMessageItem<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        let fut = Handler::handle(act, self.msg.take().unwrap(), ctx);
        fut.handle::<()>(ctx, None);
        Ok(Async::Ready(()))
    }
}

pub(crate)
struct ActorMessageStreamItem<A, M, S> where A: Actor, M: ResponseType {
    stream: S,
    act: PhantomData<A>,
    msg: PhantomData<M>,
}

impl<A, M, S> ActorMessageStreamItem<A, M, S> where A: Actor, M: ResponseType {
    pub fn new(st: S) -> Self {
        ActorMessageStreamItem { stream: st, act: PhantomData, msg: PhantomData }
    }
}

impl<A, M: 'static, S> ActorFuture for ActorMessageStreamItem<A, M, S>
    where S: Stream<Item=M, Error=()>,
          A: Actor + Handler<M>, A::Context: AsyncContext<A>,
          M: ResponseType
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    let fut = Handler::handle(act, msg, ctx);
                    fut.handle::<()>(ctx, None);
                    if ctx.waiting() {
                        return Ok(Async::NotReady)
                    }
                }
                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(_) => (),
            }
        }
    }
}
