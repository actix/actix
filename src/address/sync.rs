use std::time::{Duration, Instant};

use futures::sync::oneshot;
use futures::{Async, Future, Poll};
use tokio_timer::Delay;

use actor::Actor;
use handler::{Handler, Message};

use super::envelope::{Envelope, MessageEnvelope, ToEnvelope};
use super::sync_channel::{AddressSender, Sender};
use super::{
    Destination, MailboxError, MessageDestination, MessageRecipient, SendError,
};
use super::{Recipient, Request};

/// Sync destination of the actor. Actor can run in different thread
pub struct Syn;

impl<A: Actor> Destination<A> for Syn {
    type Transport = AddressSender<A>;

    /// Indicates if actor is still alive
    fn connected(tx: &Self::Transport) -> bool {
        tx.connected()
    }
}

impl<A: Actor, M> MessageDestination<A, M> for Syn
where
    A: Handler<M>,
    A::Context: ToEnvelope<Self, A, M>,
    M: Message + Send + 'static,
    M::Result: Send,
{
    type Envelope = Envelope<A>;
    type ResultSender = oneshot::Sender<M::Result>;
    type ResultReceiver = oneshot::Receiver<M::Result>;

    fn do_send(tx: &Self::Transport, msg: M) {
        let _ = tx.do_send(msg);
    }

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.try_send(msg, false)
    }

    fn send(tx: &Self::Transport, msg: M) -> Request<Self, A, M> {
        match tx.send(msg) {
            Ok(rx) => Request::new(Some(rx), None),
            Err(SendError::Full(msg)) => Request::new(None, Some((tx.clone(), msg))),
            Err(SendError::Closed(_)) => Request::new(None, None),
        }
    }

    fn recipient(tx: Self::Transport) -> Recipient<Self, M> {
        Recipient::new(tx.into_sender())
    }
}

impl<M> MessageRecipient<M> for Syn
where
    M: Message + Send + 'static,
    M::Result: Send,
{
    type Transport = Box<Sender<M>>;
    type Envelope = MessageEnvelope<M>;

    type SendError = SendError<M>;
    type MailboxError = MailboxError;
    type Request = RecipientRequest<M>;

    fn do_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.do_send(msg)
    }

    fn try_send(tx: &Self::Transport, msg: M) -> Result<(), SendError<M>> {
        tx.try_send(msg)
    }

    fn send(tx: &Self::Transport, msg: M) -> RecipientRequest<M> {
        match tx.send(msg) {
            Ok(rx) => RecipientRequest::new(Some(rx), None),
            Err(SendError::Full(msg)) => {
                RecipientRequest::new(None, Some((tx.boxed(), msg)))
            }
            Err(SendError::Closed(_)) => RecipientRequest::new(None, None),
        }
    }

    fn clone(tx: &Self::Transport) -> Self::Transport {
        tx.boxed()
    }
}

/// `RecipientRequest` is a `Future` which represents asynchronous message
/// sending process.
#[must_use = "future do nothing unless polled"]
pub struct RecipientRequest<M>
where
    M: Message + Send + 'static,
    M::Result: Send,
{
    rx: Option<oneshot::Receiver<M::Result>>,
    info: Option<(Box<Sender<M>>, M)>,
    timeout: Option<Delay>,
}

impl<M> RecipientRequest<M>
where
    M: Message + Send + 'static,
    M::Result: Send,
{
    pub fn new(
        rx: Option<oneshot::Receiver<M::Result>>, info: Option<(Box<Sender<M>>, M)>,
    ) -> RecipientRequest<M> {
        RecipientRequest {
            rx,
            info,
            timeout: None,
        }
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(Delay::new(Instant::now() + dur));
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
