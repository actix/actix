use std::time::Duration;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::Receiver;
use tokio_core::reactor::Timeout;

use arbiter::Arbiter;
use actor::{Actor, AsyncContext};
use fut::ActorFuture;
use handler::{Handler, MessageResult, ResponseType};

use super::{SendError, MailboxError};
use super::unsync_channel::{UnsyncAddrSender, UnsyncSender};


/// `LocalRequest` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct UnsyncRequest<A, B, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A> ,
          B: Actor, B::Context: AsyncContext<B>,
          M: ResponseType + 'static
{
    rx: Option<Receiver<Result<M::Item, M::Error>>>,
    info: Option<(UnsyncAddrSender<A>, M)>,
    act: PhantomData<B>,
    timeout: Option<Timeout>,
}

impl<A, B, M> UnsyncRequest<A, B, M>
    where A: Actor + Handler<M>, M: ResponseType + 'static,
          A::Context: AsyncContext<A>,
          B: Actor, B::Context: AsyncContext<B>
{
    pub(crate) fn new(rx: Option<Receiver<Result<M::Item, M::Error>>>,
                      info: Option<(UnsyncAddrSender<A>, M)>) -> UnsyncRequest<A, B, M> {
        UnsyncRequest{rx: rx, info: info, act: PhantomData, timeout: None}
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

impl<A, B, M> ActorFuture for UnsyncRequest<A, B, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
          B: Actor, B::Context: AsyncContext<B>,
{
    type Item = MessageResult<M>;
    type Error = MailboxError;
    type Actor = B;

    fn poll(&mut self, _: &mut B, _: &mut B::Context) -> Poll<Self::Item, Self::Error> {
        // send message
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    self.info = Some((sender, msg));
                    return self.poll_timeout();
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

/// `UnsyncFutRequest` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct UnsyncFutRequest<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
{
    rx: Option<Receiver<Result<M::Item, M::Error>>>,
    info: Option<(UnsyncAddrSender<A>, M)>,
    timeout: Option<Timeout>,
}

impl<A, M> UnsyncFutRequest<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
{
    pub(crate) fn new(rx: Option<Receiver<Result<M::Item, M::Error>>>,
                      info: Option<(UnsyncAddrSender<A>, M)>) -> UnsyncFutRequest<A, M> {
        UnsyncFutRequest{rx: rx, info: info, timeout: None}
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

impl<A, M> Future for UnsyncFutRequest<A, M>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>, M: ResponseType + 'static,
{
    type Item = MessageResult<M>;
    type Error = MailboxError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // send message
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    self.info = Some((sender, msg));
                    return self.poll_timeout();
                },
                Err(_) => return Err(MailboxError::Closed),
            }
        }

        if let Some(mut rx) = self.rx.take() {
            match rx.poll() {
                Ok(Async::Ready(item)) => Ok(Async::Ready(item)),
                Ok(Async::NotReady) => {
                    self.rx = Some(rx);
                    self.poll_timeout()
                },
                Err(_) => Err(MailboxError::Closed),
            }
        } else {
            Err(MailboxError::Closed)
        }
    }
}

/// `UnsyncSunscriberRequest` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct UnsyncSubscriberRequest<M> where M: ResponseType + 'static,
{
    rx: Option<Receiver<MessageResult<M>>>,
    info: Option<(Box<UnsyncSender<M>>, M)>,
    timeout: Option<Timeout>,
}

impl<M> UnsyncSubscriberRequest<M> where M: ResponseType + 'static,
{
    pub(crate) fn new(rx: Option<Receiver<MessageResult<M>>>,
                      info: Option<(Box<UnsyncSender<M>>, M)>) -> UnsyncSubscriberRequest<M> {
        UnsyncSubscriberRequest{rx: rx, info: info, timeout: None}
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

impl<M> Future for UnsyncSubscriberRequest<M> where M: ResponseType + 'static,
{
    type Item = MessageResult<M>;
    type Error = MailboxError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // send message
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    self.info = Some((sender, msg));
                    return self.poll_timeout();
                },
                Err(_) => return Err(MailboxError::Closed),
            }
        }

        if let Some(mut rx) = self.rx.take() {
            match rx.poll() {
                Ok(Async::Ready(item)) => Ok(Async::Ready(item)),
                Ok(Async::NotReady) => {
                    self.rx = Some(rx);
                    self.poll_timeout()
                },
                Err(_) => Err(MailboxError::Closed),
            }
        } else {
            Err(MailboxError::Closed)
        }
    }
}
