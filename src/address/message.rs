use std::time::Duration;
use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use futures::sync::oneshot::Receiver;
use tokio_core::reactor::Timeout;

use arbiter::Arbiter;
use actor::{Actor, AsyncContext};
use fut::ActorFuture;
use handler::{Handler, Message};

use super::{ToEnvelope, SendError, MailboxError};
use super::sync_channel::SyncSender;
use super::{MessageDestination, DestinationSender};


/// `Request` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct Request<T, B, M>
    where T: MessageDestination<M>,
          T::Actor: Handler<M>,
          T::Transport: DestinationSender<T, M>,
         <T::Actor as Actor>::Context: ToEnvelope<T, M>,
          B: Actor,
          M: Message + 'static
{
    rx: Option<T::ResultReceiver>,
    info: Option<(T::Transport, M)>,
    act: PhantomData<B>,
    timeout: Option<Timeout>,
}

impl<T, B, M> Request<T, B, M>
    where T: MessageDestination<M>, <T::Actor as Actor>::Context: ToEnvelope<T, M>,
          T::Actor: Handler<M>,
          T::Transport: DestinationSender<T, M>,
          B: Actor, M: Message + 'static,
{
    pub(crate) fn new(rx: Option<T::ResultReceiver>,
                      info: Option<(T::Transport, M)>) -> Request<T, B, M> {
        Request{rx: rx, info: info, act: PhantomData, timeout: None}
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

impl<T, B, M> ActorFuture for Request<T, B, M>
    where T: MessageDestination<M>,
          T::Actor: Handler<M>,
          T::Transport: DestinationSender<T, M>,
          B: Actor, B::Context: AsyncContext<B>,
          M: Message + Send + 'static, M::Result: Send,
         <T::Actor as Actor>::Context: ToEnvelope<T, M>,
{
    type Item = M::Result;
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
pub struct RequestFut<T, M>
    where T: MessageDestination<M>,
          T::Actor: Handler<M>,
          T::Transport: DestinationSender<T, M>,
         <T::Actor as Actor>::Context: ToEnvelope<T, M>,
          M: Message + 'static,
{
    rx: Option<T::ResultReceiver>,
    info: Option<(T::Transport, M)>,
    timeout: Option<Timeout>,
}

impl<T, M> RequestFut<T, M>
    where T: MessageDestination<M>,
          T::Actor: Handler<M>,
          T::Transport: DestinationSender<T, M>,
          <T::Actor as Actor>::Context: ToEnvelope<T, M>,
          M: Message + 'static,
{
    pub(crate) fn new(rx: Option<T::ResultReceiver>,
                      info: Option<(T::Transport, M)>) -> RequestFut<T, M> {
        RequestFut{rx: rx, info: info, timeout: None}
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

impl<T, M> Future for RequestFut<T, M>
    where T: MessageDestination<M>,
          T::Actor: Handler<M>,
          T::Transport: DestinationSender<T, M>,
         <T::Actor as Actor>::Context: ToEnvelope<T, M>,
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
pub struct SyncSubscriberRequest<M>
    where M: Message + Send + 'static, M::Result: Send,
{
    rx: Option<Receiver<M::Result>>,
    info: Option<(Box<SyncSender<M>>, M)>,
    timeout: Option<Timeout>,
}

impl<M> SyncSubscriberRequest<M>
    where M: Message + Send + 'static, M::Result: Send,
{
    pub(crate) fn new(rx: Option<Receiver<M::Result>>,
                      info: Option<(Box<SyncSender<M>>, M)>) -> SyncSubscriberRequest<M> {
        SyncSubscriberRequest{rx: rx, info: info, timeout: None}
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

impl<M> Future for SyncSubscriberRequest<M>
    where M: Message + Send + 'static, M::Result: Send,
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
