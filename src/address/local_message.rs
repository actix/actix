use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::{Canceled, Receiver};

use actor::{Actor, AsyncContext};
use fut::ActorFuture;
use handler::{Handler, ResponseType};

use super::SendError;
use super::local_channel::LocalAddrSender;


/// `LocalRequest` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct LocalRequest<A, B, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A> ,
          B: Actor, B::Context: AsyncContext<B>,
          M: ResponseType + 'static
{
    rx: Option<Receiver<Result<M::Item, M::Error>>>,
    info: Option<(LocalAddrSender<A>, M)>,
    act: PhantomData<B>,
}

impl<A, B, M> LocalRequest<A, B, M>
    where A: Actor + Handler<M>, M: ResponseType + 'static,
          A::Context: AsyncContext<A>,
          B: Actor, B::Context: AsyncContext<B>
{
    pub(crate) fn new(rx: Option<Receiver<Result<M::Item, M::Error>>>,
                      info: Option<(LocalAddrSender<A>, M)>) -> LocalRequest<A, B, M> {
        LocalRequest{rx: rx, info: info, act: PhantomData}
    }
}

impl<A, B, M> ActorFuture for LocalRequest<A, B, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
          B: Actor, B::Context: AsyncContext<B>,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;
    type Actor = B;

    fn poll(&mut self, _: &mut B, _: &mut B::Context) -> Poll<Self::Item, Self::Error> {
        // send message
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::NotReady(msg)) => {
                    self.info = Some((sender, msg));
                    return Ok(Async::NotReady)
                }
                Err(SendError::Closed(_)) => return Err(Canceled),
            }
        }

        if let Some(ref mut rx) = self.rx {
            rx.poll()
        } else {
            Err(Canceled)
        }
    }
}

/// `LocalFutRequest` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct LocalFutRequest<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
{
    rx: Option<Receiver<Result<M::Item, M::Error>>>,
    info: Option<(LocalAddrSender<A>, M)>,
}

impl<A, M> LocalFutRequest<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
{
    pub(crate) fn new(rx: Option<Receiver<Result<M::Item, M::Error>>>,
                      info: Option<(LocalAddrSender<A>, M)>) -> LocalFutRequest<A, M> {
        LocalFutRequest{rx: rx, info: info}
    }
}

impl<A, M> Future for LocalFutRequest<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // send message
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::NotReady(msg)) => {
                    self.info = Some((sender, msg));
                    return Ok(Async::NotReady)
                },
                Err(_) => return Err(Canceled),
            }
        }

        if let Some(ref mut rx) = self.rx {
            rx.poll()
        } else {
            Err(Canceled)
        }
    }
}
