use std::marker::PhantomData;
use futures::{Async, Future, Poll, Stream};

use fut::ActorFuture;
use actor::{Actor, AsyncContext};
use handler::{Handler, ResponseType, IntoResponse, StreamHandler};
use message::Response;

pub(crate)
struct ActorDelayedMessageCell<A, M, F>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=()>,
{
    act: PhantomData<A>,
    fut: F,
    result: Option<Response<A, M>>,
}

impl<A, M, F> ActorDelayedMessageCell<A, M, F>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=()>,
{
    pub fn new(fut: F) -> ActorDelayedMessageCell<A, M, F>
    {
        ActorDelayedMessageCell {
            act: PhantomData,
            fut: fut,
            result: None,
        }
    }
}

impl<A, M, F> ActorFuture for ActorDelayedMessageCell<A, M, F>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=()>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error>
    {
        loop {
            if let Some(mut fut) = self.result.take() {
                match fut.poll_response(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.result = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) =>
                        return Ok(Async::Ready(())),
                    Err(_) =>
                        return Err(())
                }
            }

            match self.fut.poll() {
                Ok(Async::Ready(msg)) => {
                    let fut = <Self::Actor as Handler<M>>::handle(act, msg, ctx);
                    self.result = Some(fut.into_response());
                    continue
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(_) => {
                    error!("Delayed message got error");
                    return Err(())
                }
            }
        }
    }
}

pub(crate)
struct ActorMessageCell<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    act: PhantomData<A>,
    msg: Option<M>,
    result: Option<Response<A, M>>,
}

impl<A, M> ActorMessageCell<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    pub fn new(msg: M) -> ActorMessageCell<A, M>
    {
        ActorMessageCell {
            act: PhantomData,
            msg: Some(msg),
            result: None,
        }
    }
}

impl<A, M> ActorFuture for ActorMessageCell<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error>
    {
        loop {
            if let Some(mut fut) = self.result.take() {
                match fut.poll_response(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.result = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) =>
                        return Ok(Async::Ready(())),
                    Err(_) =>
                        return Err(())
                }
            }

            let fut = <Self::Actor as Handler<M>>::handle(act, self.msg.take().unwrap(), ctx);
            self.result = Some(fut.into_response());
            continue
        }
    }
}

pub(crate)
struct ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + StreamHandler<Result<M, E>>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    act: PhantomData<A>,
    started: bool,
    fut: Option<Response<A, Result<M, E>>>,
    stream: S,
}

impl<A, M, E, S> ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E> + 'static,
          A: Actor + StreamHandler<Result<M, E>>,
          A::Context: AsyncContext<A>,
          M: ResponseType
{
    pub fn new(fut: S) -> ActorStreamCell<A, M, E, S>
    {
        ActorStreamCell {
            act: PhantomData,
            started: false,
            fut: None,
            stream: fut }
    }
}

impl<A, M, E, S> ActorFuture for ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + Handler<Result<M, E>> + StreamHandler<Result<M, E>>,
          A::Context: AsyncContext<A>,
          M: ResponseType
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error>
    {
        if !self.started {
            self.started = true;
            <A as StreamHandler<Result<M, E>>>::started(act, ctx);
        }

        loop {
            if let Some(mut fut) = self.fut.take() {
                match fut.poll_response(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.fut = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) => (),
                    Err(_) => return Err(())
                }
            }

            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    let fut = <Self::Actor as Handler<Result<M, E>>>::handle(act, Ok(msg), ctx);
                    self.fut = Some(fut.into_response());
                    continue
                }
                Ok(Async::Ready(None)) => {
                    <A as StreamHandler<Result<M, E>>>::finished(act, ctx);
                    return Ok(Async::Ready(()))
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(err) => {
                    <Self::Actor as Handler<Result<M, E>>>::handle(act, Err(err), ctx);
                    continue
                }
            }
        }
    }
}

pub(crate)
struct ActorMessageStreamCell<A, M, S>
    where S: Stream<Item=M, Error=()>,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    act: PhantomData<A>,
    fut: Option<Response<A, M>>,
    stream: S,
}

impl<A, M, S> ActorMessageStreamCell<A, M, S>
    where S: Stream<Item=M, Error=()> + 'static,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType
{
    pub fn new(fut: S) -> ActorMessageStreamCell<A, M, S>
    {
        ActorMessageStreamCell {
            act: PhantomData,
            fut: None,
            stream: fut,
        }
    }
}

impl<A, M, S> ActorFuture for ActorMessageStreamCell<A, M, S>
    where S: Stream<Item=M, Error=()>,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error>
    {
        loop {
            if let Some(mut fut) = self.fut.take() {
                match fut.poll_response(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.fut = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) => (),
                    Err(_) => return Err(())
                }
            }

            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    let fut = <A as Handler<M>>::handle(act, msg, ctx);
                    self.fut = Some(fut.into_response());
                    continue
                }
                Ok(Async::Ready(None)) =>
                    return Ok(Async::Ready(())),
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(_) =>
                    continue,
            }
        }
    }
}
