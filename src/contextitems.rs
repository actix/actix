use std::marker::PhantomData;
use futures::{Async, Future, Poll, Stream};

use fut::ActorFuture;
use actor::{Actor, AsyncContext};
use handler::{Handler, ResponseType, IntoResponse};
use message::Response;


pub(crate)
struct ActorFutureItem<A, M, F, E>
    where A: Actor + Handler<Result<M, E>>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=E>,
{
    act: PhantomData<A>,
    fut: F,
    result: Option<Response<A, Result<M, E>>>,
}

impl<A, M, F, E> ActorFutureItem<A, M, F, E>
    where A: Actor + Handler<Result<M, E>>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=E>,
{
    pub fn new(fut: F) -> ActorFutureItem<A, M, F, E> {
        ActorFutureItem {
            act: PhantomData,
            fut: fut,
            result: None,
        }
    }
}

impl<A, M, F, E> ActorFuture for ActorFutureItem<A, M, F, E>
    where A: Actor + Handler<Result<M, E>>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=E>,
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
                    Ok(Async::Ready(_)) => return Ok(Async::Ready(())),
                    Err(_) => return Err(()),
                }
            }

            match self.fut.poll() {
                Ok(Async::Ready(msg)) => {
                    let fut = <A as Handler<Result<M, E>>>::handle(act, Ok(msg), ctx);
                    self.result = Some(fut.into_response());
                },
                Err(err) => {
                    let fut = <A as Handler<Result<M, E>>>::handle(act, Err(err), ctx);
                    self.result = Some(fut.into_response());
                },
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
            }
        }
    }
}

pub(crate)
struct ActorDelayedMessageItem<A, M, F>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=()> {
    act: PhantomData<A>,
    fut: F,
    result: Option<Response<A, M>>,
}

impl<A, M, F> ActorDelayedMessageItem<A, M, F>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=()>,
{
    pub fn new(fut: F) -> ActorDelayedMessageItem<A, M, F> {
        ActorDelayedMessageItem {
            act: PhantomData,
            fut: fut,
            result: None,
        }
    }
}

impl<A, M, F> ActorFuture for ActorDelayedMessageItem<A, M, F>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
          F: Future<Item=M, Error=()>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        loop {
            if let Some(mut fut) = self.result.take() {
                match fut.poll_response(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.result = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) => return Ok(Async::Ready(())),
                    Err(_) => return Err(()),
                }
            }

            match self.fut.poll() {
                Ok(Async::Ready(msg)) => {
                    let fut = <A as Handler<M>>::handle(act, msg, ctx);
                    self.result = Some(fut.into_response());
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(_) => {
                    error!("Delayed message got error");
                    return Err(())
                },
            }
        }
    }
}

pub(crate)
struct ActorMessageItem<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    act: PhantomData<A>,
    msg: Option<M>,
    result: Option<Response<A, M>>,
}

impl<A, M> ActorMessageItem<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    pub fn new(msg: M) -> ActorMessageItem<A, M> {
        ActorMessageItem {
            act: PhantomData,
            msg: Some(msg),
            result: None,
        }
    }
}

impl<A, M> ActorFuture for ActorMessageItem<A, M>
    where A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
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
        }
    }
}

pub(crate)
struct ActorStreamItem<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + Handler<Result<M, E>>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    act: PhantomData<A>,
    fut: Option<Response<A, Result<M, E>>>,
    stream: S,
}

impl<A, M, E, S> ActorStreamItem<A, M, E, S>
    where S: Stream<Item=M, Error=E> + 'static,
          A: Actor + Handler<Result<M, E>>,
          A::Context: AsyncContext<A>,
          M: ResponseType
{
    pub fn new(fut: S) -> ActorStreamItem<A, M, E, S> {
        ActorStreamItem {
            act: PhantomData,
            fut: None,
            stream: fut }
    }
}

impl<A, M, E, S> ActorFuture for ActorStreamItem<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + Handler<Result<M, E>>,
          A::Context: AsyncContext<A>,
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
                    Ok(Async::Ready(_)) => (),
                    Err(_) => return Err(())
                }
            }

            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    let fut = <A as Handler<Result<M, E>>>::handle(act, Ok(msg), ctx);
                    self.fut = Some(fut.into_response());
                }
                Err(err) => {
                    let fut = <A as Handler<Result<M, E>>>::handle(act, Err(err), ctx);
                    self.fut = Some(fut.into_response());
                },
                Ok(Async::Ready(None)) =>
                    return Ok(Async::Ready(())),
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
            }
        }
    }
}

pub(crate)
struct ActorMessageStreamItem<A, M, S>
    where S: Stream<Item=M, Error=()>,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType,
{
    act: PhantomData<A>,
    fut: Option<Response<A, M>>,
    stream: S,
}

impl<A, M, S> ActorMessageStreamItem<A, M, S>
    where S: Stream<Item=M, Error=()> + 'static,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
          M: ResponseType
{
    pub fn new(fut: S) -> ActorMessageStreamItem<A, M, S> {
        ActorMessageStreamItem {
            act: PhantomData,
            fut: None,
            stream: fut,
        }
    }
}

impl<A, M, S> ActorFuture for ActorMessageStreamItem<A, M, S>
    where S: Stream<Item=M, Error=()>,
          A: Actor + Handler<M>,
          A::Context: AsyncContext<A>,
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
                    Ok(Async::Ready(_)) => (),
                    Err(_) => return Err(())
                }
            }

            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    let fut = <A as Handler<M>>::handle(act, msg, ctx);
                    self.fut = Some(fut.into_response());
                }
                Ok(Async::Ready(None)) =>
                    return Ok(Async::Ready(())),
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(_) => (),
            }
        }
    }
}
