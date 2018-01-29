use std::time::Duration;
use futures::{Async, Future, Poll, Stream};
use tokio_core::reactor::Timeout;

use fut::ActorFuture;
use arbiter::Arbiter;
use actor::{Actor, ActorContext, AsyncContext};
use handler::{Handler, Response, ResponseType, IntoResponse};


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


pub(crate) struct ActorFutureItem<A, M, F, E> where A: Actor, M: ResponseType {
    fut: F,
    result: Option<Response<A, Result<M, E>>>,
}

impl<A, M, F, E> ActorFutureItem<A, M, F, E> where A: Actor, M: ResponseType {
    pub fn new(fut: F) -> Self {
        ActorFutureItem {fut: fut, result: None}
    }
}

impl<A, M, F, E> ActorFuture for ActorFutureItem<A, M, F, E>
    where A: Actor + Handler<Result<M, E>>, A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=E>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        if self.result.is_none() {
            match self.fut.poll() {
                Ok(Async::Ready(msg)) => {
                    let fut = <A as Handler<Result<M, E>>>::handle(act, Ok(msg), ctx);
                    self.result = Some(fut.into_response());
                },
                Err(err) => {
                    let fut = <A as Handler<Result<M, E>>>::handle(act, Err(err), ctx);
                    self.result = Some(fut.into_response());
                },
                Ok(Async::NotReady) => return Ok(Async::NotReady),
            }
        }

        match self.result.as_mut().unwrap().poll_response(act, ctx) {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => Ok(Async::Ready(())),
            Err(_) => Err(()),
        }
    }
}

pub(crate) struct ActorDelayedMessageItem<A, M> where A: Actor, M: ResponseType {
    msg: Option<M>,
    timeout: Timeout,
    result: Option<Response<A, M>>,
}

impl<A, M> ActorDelayedMessageItem<A, M> where A: Actor, M: ResponseType {
    pub fn new(msg: M, timeout: Duration) -> Self {
        ActorDelayedMessageItem {
            msg: Some(msg),
            timeout: Timeout::new(timeout, Arbiter::handle()).unwrap(),
            result: None,
        }
    }
}

impl<A, M> ActorFuture for ActorDelayedMessageItem<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        if self.result.is_none() {
            match self.timeout.poll() {
                Ok(Async::Ready(_)) => {
                    let fut = A::handle(act, self.msg.take().unwrap(), ctx);
                    self.result = Some(fut.into_response());
                },
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(_) => unreachable!(),
            }
        }

        match self.result.as_mut().unwrap().poll_response(act, ctx) {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => Ok(Async::Ready(())),
            Err(_) => Err(()),
        }
    }
}

pub(crate) struct ActorMessageItem<A, M> where A: Actor, M: ResponseType {
    msg: Option<M>,
    result: Option<Response<A, M>>,
}

impl<A, M> ActorMessageItem<A, M> where A: Actor, M: ResponseType {
    pub fn new(msg: M) -> Self {
        ActorMessageItem{msg: Some(msg), result: None}
    }
}

impl<A, M> ActorFuture for ActorMessageItem<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        if self.result.is_none() {
            let fut = Handler::handle(act, self.msg.take().unwrap(), ctx);
            self.result = Some(fut.into_response());
        }

        match self.result.as_mut().unwrap().poll_response(act, ctx) {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => Ok(Async::Ready(())),
            Err(_) => Err(())
        }
    }
}

pub(crate) struct ActorMessageStreamItem<A, M, S> where A: Actor, M: ResponseType {
    stream: S,
    fut: Option<Response<A, M>>,
}

impl<A, M, S> ActorMessageStreamItem<A, M, S> where A: Actor, M: ResponseType {
    pub fn new(st: S) -> Self {
        ActorMessageStreamItem { fut: None, stream: st }
    }
}

impl<A, M, S> ActorFuture for ActorMessageStreamItem<A, M, S>
    where S: Stream<Item=M, Error=()>,
          A: Actor + Handler<M>, A::Context: AsyncContext<A>,
          M: ResponseType
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        loop {
            if let Some(mut fut) = self.fut.take() {
                match fut.poll_response(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.fut = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) => if ctx.waiting() {
                        return Ok(Async::NotReady)
                    },
                    Err(_) => return Err(())
                }
            }

            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    let fut = Handler::handle(act, msg, ctx);
                    self.fut = Some(fut.into_response());
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
