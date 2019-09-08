use std::marker::PhantomData;
use std::time::Duration;

use futures::sync::oneshot;
use futures::{Async, Future, Poll};
use tokio_timer::Delay;

use crate::clock;
use crate::handler::{Handler, Message};

use super::channel::{AddressSender, Sender};
use super::{MailboxError, SendError, ToEnvelope};

/// A `Future` which represents an asynchronous message sending
/// process.
#[must_use = "You have to wait on request otherwise the Message wont be delivered"]
pub struct Request<A, M>
where
    A: Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message,
{
    rx: Option<oneshot::Receiver<M::Result>>,
    info: Option<(AddressSender<A>, M)>,
    timeout: Option<Delay>,
    act: PhantomData<A>,
}

impl<A, M> Request<A, M>
where
    A: Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message,
{
    pub(crate) fn new(
        rx: Option<oneshot::Receiver<M::Result>>,
        info: Option<(AddressSender<A>, M)>,
    ) -> Request<A, M> {
        Request {
            rx,
            info,
            timeout: None,
            act: PhantomData,
        }
    }

    #[cfg(test)]
    pub(crate) fn rx_is_some(&self) -> bool {
        self.rx.is_some()
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(Delay::new(clock::now() + dur));
        self
    }

    fn poll_timeout(&mut self) -> Poll<M::Result, MailboxError> {
        if let Some(ref mut timeout) = self.timeout {
            match timeout.poll() {
                Ok(Async::Ready(())) => Err(MailboxError::Timeout),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(_) => unreachable!(),
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl<A, M> Future for Request<A, M>
where
    A: Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M: Message + Send,
    M::Result: Send,
{
    type Item = M::Result;
    type Error = MailboxError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    self.info = Some((sender, msg));
                    return Ok(Async::NotReady);
                }
                Err(SendError::Closed(_)) => return Err(MailboxError::Closed),
            }
        }

        if self.rx.is_some() {
            match self.rx.as_mut().unwrap().poll() {
                Ok(Async::Ready(item)) => Ok(Async::Ready(item)),
                Ok(Async::NotReady) => self.poll_timeout(),
                Err(_) => Err(MailboxError::Closed),
            }
        } else {
            Err(MailboxError::Closed)
        }
    }
}

/// A `Future` which represents an asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct RecipientRequest<M>
where
    M: Message + Send + 'static,
    M::Result: Send,
{
    rx: Option<oneshot::Receiver<M::Result>>,
    info: Option<(Box<dyn Sender<M>>, M)>,
    timeout: Option<Delay>,
}

impl<M> RecipientRequest<M>
where
    M: Message + Send + 'static,
    M::Result: Send,
{
    pub fn new(
        rx: Option<oneshot::Receiver<M::Result>>,
        info: Option<(Box<dyn Sender<M>>, M)>,
    ) -> RecipientRequest<M> {
        RecipientRequest {
            rx,
            info,
            timeout: None,
        }
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(Delay::new(clock::now() + dur));
        self
    }

    fn poll_timeout(&mut self) -> Poll<M::Result, MailboxError> {
        if let Some(ref mut timeout) = self.timeout {
            match timeout.poll() {
                Ok(Async::Ready(())) => Err(MailboxError::Timeout),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(_) => unreachable!(),
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl<M> Future for RecipientRequest<M>
where
    M: Message + Send + 'static,
    M::Result: Send,
{
    type Item = M::Result;
    type Error = MailboxError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some((sender, msg)) = self.info.take() {
            match sender.send(msg) {
                Ok(rx) => self.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    self.info = Some((sender, msg));
                    return Ok(Async::NotReady);
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
