use futures::unsync::oneshot::{Receiver, Sender};
use futures::{Async, Future, Poll};
use std::time::Duration;
use tokio_core::reactor::Timeout;

use actor::{Actor, AsyncContext};
use arbiter::Arbiter;
use handler::{Handler, Message};

use super::unsync_channel::{UnsyncAddrSender, UnsyncSender};
use super::{Destination, MailboxError, MessageDestination, MessageRecipient, SendError};
use super::{MessageEnvelope, ToEnvelope, UnsyncEnvelope};
use super::{Recipient, Request};

/// Unsync destination of the actor
///
/// Actor has to run in the same thread as owner of the address.
pub struct Unsync;

impl<A: Actor> Destination<A> for Unsync
where
    A::Context: AsyncContext<A>,
{
    type Transport = UnsyncAddrSender<A>;

    /// Indicates if actor is still alive
    fn connected(tx: &Self::Transport) -> bool {
        tx.connected()
    }
}

impl<A, M> MessageDestination<A, M> for Unsync
where
    M: Message + 'static,
    A: Handler<M>,
    A::Context: AsyncContext<A> + ToEnvelope<Self, A, M>,
{
    type Envelope = UnsyncEnvelope<A>;
    type ResultSender = Sender<M::Result>;
    type ResultReceiver = Receiver<M::Result>;

    fn do_send(tx: &Self::Transport, msg: M) {
        let _ = tx.do_send(msg);
    }

    fn send(tx: &Self::Transport, msg: M) -> Request<Self, A, M> {
        match tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::Full(msg)) => Request::new(None, Some((tx.clone(), msg))),
            Err(SendError::Closed(_)) => Request::new(None, None),
        }
    }

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.try_send(msg, false)
    }

    fn recipient(tx: Self::Transport) -> Recipient<Self, M> {
        Recipient::new(tx.into_sender())
    }
}

impl<M> MessageRecipient<M> for Unsync
where
    M: Message + 'static,
{
    type Envelope = MessageEnvelope<M>;
    type Transport = Box<UnsyncSender<M>>;

    type SendError = SendError<M>;
    type MailboxError = MailboxError;
    type Request = UnsyncRecipientRequest<M>;

    fn do_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.do_send(msg)
    }

    fn send(tx: &Self::Transport, msg: M) -> UnsyncRecipientRequest<M> {
        match tx.send(msg) {
            Ok(rx) => UnsyncRecipientRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) => {
                UnsyncRecipientRequest::new(None, Some((tx.boxed(), msg)))
            }
            Err(SendError::Closed(_)) => UnsyncRecipientRequest::new(None, None),
        }
    }

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.try_send(msg)
    }

    fn clone(tx: &Self::Transport) -> Self::Transport {
        tx.boxed()
    }
}

/// `UnsyncRecipientRequest` is a `Future` which represents asynchronous
/// message sending process.
#[must_use = "future do nothing unless polled"]
pub struct UnsyncRecipientRequest<M>
where
    M: Message + 'static,
{
    rx: Option<Receiver<M::Result>>,
    info: Option<(Box<UnsyncSender<M>>, M)>,
    timeout: Option<Timeout>,
}

impl<M> UnsyncRecipientRequest<M>
where
    M: Message + 'static,
{
    pub fn new(
        rx: Option<Receiver<M::Result>>, info: Option<(Box<UnsyncSender<M>>, M)>
    ) -> UnsyncRecipientRequest<M> {
        UnsyncRecipientRequest {
            rx,
            info,
            timeout: None,
        }
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
                Err(_) => unreachable!(),
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl<M> Future for UnsyncRecipientRequest<M>
where
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
