use std::time::Duration;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use tokio_core::reactor::Timeout;

use arbiter::Arbiter;
use handler::{Handler, Message};

use super::{ToEnvelope, SendError, MailboxError};
use super::{DestinationSender, MessageDestination, MessageSubscriber, MessageSubscriberSender};


/// `Request` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<T, A, M>
    where T: MessageDestination<A, M>,
          A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
          T::Transport: DestinationSender<T, A, M>,
          M: Message + 'static,
{
    rx: Option<T::ResultReceiver>,
    info: Option<(T::Transport, M)>,
    timeout: Option<Timeout>,
    act: PhantomData<A>,
}

impl<T, A, M> Request<T, A, M>
    where T: MessageDestination<A, M>,
          T::Transport: DestinationSender<T, A, M>,
          A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
          M: Message + 'static,
{
    pub(crate) fn new(rx: Option<T::ResultReceiver>,
                      info: Option<(T::Transport, M)>) -> Request<T, A, M> {
        Request{rx: rx, info: info, timeout: None, act: PhantomData}
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(Timeout::new(dur, Arbiter::handle()).unwrap());
        self
    }

    fn poll_timeout(&mut self) -> Poll<M::Result, MailboxError> {
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

impl<T, A, M> Future for Request<T, A, M>
    where T: MessageDestination<A, M>,
          T::Transport: DestinationSender<T, A, M>,
          A: Handler<M>, A::Context: ToEnvelope<T, A, M>,
          M: Message + 'static,
{
    type Item = M::Result;
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
pub struct SubscriberRequest<T, M>
    where T: MessageSubscriber<M>, M: Message + 'static
{
    rx: Option<T::ResultReceiver>,
    info: Option<(T::Transport, M)>,
    timeout: Option<Timeout>,
}

impl<T, M> SubscriberRequest<T, M>
    where T: MessageSubscriber<M>, M: Message + 'static
{
    pub(crate) fn new(rx: Option<T::ResultReceiver>,
                      info: Option<(T::Transport, M)>) -> SubscriberRequest<T, M> {
        SubscriberRequest{rx: rx, info: info, timeout: None}
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(Timeout::new(dur, Arbiter::handle()).unwrap());
        self
    }

    fn poll_timeout(&mut self) -> Poll<M::Result, MailboxError> {
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

impl<T, M> Future for SubscriberRequest<T, M>
    where T: MessageSubscriber<M>, M: Message + 'static
{
    type Item = M::Result;
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
