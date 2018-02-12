use std::time::Duration;

use futures::{Async, Future, Poll};
use futures::unsync::oneshot::Receiver;
use tokio_core::reactor::Timeout;

use arbiter::Arbiter;
use handler::Message;

use super::{SendError, MailboxError};
use super::unsync_channel::UnsyncSender;

/// `UnsyncSunscriberRequest` is a `Future` which represents asynchronous message sending process.
#[must_use = "future do nothing unless polled"]
pub struct UnsyncSubscriberRequest<M> where M: Message + 'static,
{
    rx: Option<Receiver<M::Result>>,
    info: Option<(Box<UnsyncSender<M>>, M)>,
    timeout: Option<Timeout>,
}

impl<M> UnsyncSubscriberRequest<M> where M: Message + 'static,
{
    pub(crate) fn new(rx: Option<Receiver<M::Result>>,
                      info: Option<(Box<UnsyncSender<M>>, M)>) -> UnsyncSubscriberRequest<M> {
        UnsyncSubscriberRequest{rx: rx, info: info, timeout: None}
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

impl<M> Future for UnsyncSubscriberRequest<M> where M: Message + 'static,
{
    type Item = M::Result;
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
