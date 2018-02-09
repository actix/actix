use std::time::Duration;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::sync::oneshot::Receiver;
use tokio_core::reactor::Timeout;

use arbiter::Arbiter;
use actor::{Actor, AsyncContext};
use fut::ActorFuture;
use handler::{Handler, ResponseType, MessageResult};

use super::{SendError, MailboxError};
use super::sync_channel::{AddressSender, SyncSender};


/// `Request` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<A, B, M> where A: Actor, B: Actor, M: ResponseType {
    rx: Option<Receiver<MessageResult<M>>>,
    info: Option<(AddressSender<A>, M)>,
    act: PhantomData<B>,
    timeout: Option<Timeout>,
}

impl<A, B, M> Request<A, B, M> where A: Actor, B: Actor, M: ResponseType
{
    pub(crate) fn new(rx: Option<Receiver<MessageResult<M>>>,
                      info: Option<(AddressSender<A>, M)>) -> Request<A, B, M> {
        Request{rx: rx, info: info, act: PhantomData, timeout: None}
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(Timeout::new(dur, Arbiter::handle()).unwrap());
        self
    }

    fn poll_timeout(&mut self) -> Poll<MessageResult<M>, MailboxError> {
        if let Some(ref mut timeout) = self.timeout {
            match timeout.poll() {
                Ok(Async::Ready(())) => Err(MailboxError::Timeout),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(_) => unreachable!()
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl<A, B, M> ActorFuture for Request<A, B, M>
    where A: Actor + Handler<M>, B: Actor, B::Context: AsyncContext<B>,
          M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
{
    type Item = MessageResult<M>;
    type Error = MailboxError;
    type Actor = B;

    fn poll(&mut self, _: &mut B, _: &mut B::Context) -> Poll<Self::Item, Self::Error> {
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    self.info = Some((sender, msg));
                    return Ok(Async::NotReady)
                }
                Err(SendError::Closed(_)) => return Err(MailboxError::Closed),
            }
        }

        if let Some(mut rx) = self.rx.take() {
            match rx.poll() {
                Ok(Async::Ready(item)) => Ok(Async::Ready(item)),
                Ok(Async::NotReady) => {
                    self.rx = Some(rx);
                    self.poll_timeout()
                }
                Err(_) => Err(MailboxError::Closed),
            }
        } else {
            Err(MailboxError::Closed)
        }
    }
}

/// `RequestFut` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct RequestFut<A, M> where A: Actor, M: ResponseType {
    rx: Option<Receiver<MessageResult<M>>>,
    info: Option<(AddressSender<A>, M)>,
    timeout: Option<Timeout>,
}

impl<A, M> RequestFut<A, M> where A: Actor, M: ResponseType
{
    pub(crate) fn new(rx: Option<Receiver<MessageResult<M>>>,
                      info: Option<(AddressSender<A>, M)>) -> RequestFut<A, M> {
        RequestFut{rx: rx, info: info, timeout: None}
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(Timeout::new(dur, Arbiter::handle()).unwrap());
        self
    }

    fn poll_timeout(&mut self) -> Poll<MessageResult<M>, MailboxError> {
        if let Some(ref mut timeout) = self.timeout {
            match timeout.poll() {
                Ok(Async::Ready(())) => Err(MailboxError::Timeout),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(_) => unreachable!()
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl<A, M> Future for RequestFut<A, M>
    where A: Actor + Handler<M>,
          M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
{
    type Item = Result<M::Item, M::Error>;
    type Error = MailboxError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    self.info = Some((sender, msg));
                    return Ok(Async::NotReady)
                }
                Err(SendError::Closed(_)) => return Err(MailboxError::Closed),
            }
        }

        if let Some(mut rx) = self.rx.take() {
            match rx.poll() {
                Ok(Async::Ready(item)) => Ok(Async::Ready(item)),
                Ok(Async::NotReady) => {
                    self.rx = Some(rx);
                    self.poll_timeout()
                }
                Err(_) => Err(MailboxError::Closed),
            }
        } else {
            Err(MailboxError::Closed)
        }
    }
}

/// `SubscriberRequest` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct SyncSubscriberRequest<M>
    where M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send
{
    rx: Option<Receiver<MessageResult<M>>>,
    info: Option<(Box<SyncSender<M>>, M)>,
    timeout: Option<Timeout>,
}

impl<M> SyncSubscriberRequest<M>
    where M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send
{
    pub(crate) fn new(rx: Option<Receiver<MessageResult<M>>>,
                      info: Option<(Box<SyncSender<M>>, M)>) -> SyncSubscriberRequest<M> {
        SyncSubscriberRequest{rx: rx, info: info, timeout: None}
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(Timeout::new(dur, Arbiter::handle()).unwrap());
        self
    }

    fn poll_timeout(&mut self) -> Poll<MessageResult<M>, MailboxError> {
        if let Some(ref mut timeout) = self.timeout {
            match timeout.poll() {
                Ok(Async::Ready(())) => Err(MailboxError::Timeout),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(_) => unreachable!()
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl<M> Future for SyncSubscriberRequest<M>
    where M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send,
{
    type Item = Result<M::Item, M::Error>;
    type Error = MailboxError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    self.info = Some((sender, msg));
                    return Ok(Async::NotReady)
                }
                Err(SendError::Closed(_)) => return Err(MailboxError::Closed),
            }
        }

        if let Some(mut rx) = self.rx.take() {
            match rx.poll() {
                Ok(Async::Ready(item)) => Ok(Async::Ready(item)),
                Ok(Async::NotReady) => {
                    self.rx = Some(rx);
                    self.poll_timeout()
                }
                Err(_) => Err(MailboxError::Closed),
            }
        } else {
            Err(MailboxError::Closed)
        }
    }
}
