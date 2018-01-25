use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::sync::oneshot::Receiver;
use futures::unsync::oneshot::Canceled;

use actor::{Actor, AsyncContext};
use fut::ActorFuture;
use address::SendError;
use handler::{Handler, ResponseType, MessageResult};

use super::channel::AddressSender;


/// `Request` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<A, B, M> where A: Actor, B: Actor, M: ResponseType {
    rx: Option<Receiver<MessageResult<M>>>,
    info: Option<(AddressSender<A>, M)>,
    act: PhantomData<B>,
}

impl<A, B, M> Request<A, B, M> where A: Actor, B: Actor, M: ResponseType
{
    pub(crate) fn new(rx: Option<Receiver<MessageResult<M>>>,
                      info: Option<(AddressSender<A>, M)>) -> Request<A, B, M> {
        Request{rx: rx, info: info, act: PhantomData}
    }
}

impl<A, B, M> ActorFuture for Request<A, B, M>
    where A: Actor + Handler<M>, B: Actor, B::Context: AsyncContext<B>,
          M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;
    type Actor = B;

    fn poll(&mut self, _: &mut B, _: &mut B::Context) -> Poll<Self::Item, Self::Error> {
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

/// `RequestFut` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct RequestFut<A, M> where A: Actor, M: ResponseType {
    rx: Option<Receiver<MessageResult<M>>>,
    info: Option<(AddressSender<A>, M)>,
}

impl<A, M> RequestFut<A, M> where A: Actor, M: ResponseType
{
    pub(crate) fn new(rx: Option<Receiver<MessageResult<M>>>,
                      info: Option<(AddressSender<A>, M)>) -> RequestFut<A, M> {
        RequestFut{rx: rx, info: info}
    }
}

impl<A, M> Future for RequestFut<A, M>
    where A: Actor + Handler<M>,
          M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
{
    type Item = Result<M::Item, M::Error>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
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
