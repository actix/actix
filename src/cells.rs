use std::marker::PhantomData;
use futures::{Async, Future, Poll};

use fut::ActorFuture;
use actor::{Actor, AsyncContext, Handler, ResponseType};
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
                    self.result = Some(fut);
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
            self.result = Some(fut);
            continue
        }
    }
}
