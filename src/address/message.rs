use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{self, Poll};
use std::time::Duration;

use futures::channel::oneshot;

use crate::clock::Delay;
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
        self.timeout = Some(tokio::time::delay_for(dur));
        self
    }

    fn poll_timeout(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<M::Result, MailboxError>> {
        if let Some(ref mut timeout) = self.timeout {
            match unsafe { Pin::new_unchecked(timeout) }.poll(cx) {
                Poll::Ready(()) => Poll::Ready(Err(MailboxError::Timeout)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Pending
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
    type Output = Result<M::Result, MailboxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let mut this = unsafe { self.get_unchecked_mut() };

        if let Some((sender, msg)) = this.info.take() {
            match sender.send(msg) {
                Ok(rx) => this.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    this.info = Some((sender, msg));
                    return Poll::Pending;
                }
                Err(SendError::Closed(_)) => {
                    return Poll::Ready(Err(MailboxError::Closed))
                }
            }
        }

        if this.rx.is_some() {
            match unsafe { Pin::new_unchecked(&mut this.rx.as_mut().unwrap()) }.poll(cx)
            {
                Poll::Ready(Ok(i)) => Poll::Ready(Ok(i)),
                Poll::Ready(Err(_)) => Poll::Ready(Err(MailboxError::Closed)),
                Poll::Pending => this.poll_timeout(cx),
            }
        } else {
            Poll::Ready(Err(MailboxError::Closed))
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
        self.timeout = Some(tokio::time::delay_for(dur));
        self
    }

    fn poll_timeout(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<M::Result, MailboxError>> {
        if let Some(ref mut timeout) = self.timeout {
            match unsafe { Pin::new_unchecked(timeout) }.poll(cx) {
                Poll::Ready(()) => Poll::Ready(Err(MailboxError::Timeout)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Pending
        }
    }
}

impl<M> Future for RecipientRequest<M>
where
    M: Message + Send + 'static,
    M::Result: Send,
{
    type Output = Result<M::Result, MailboxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let mut this = unsafe { self.get_unchecked_mut() };

        if let Some((sender, msg)) = this.info.take() {
            match sender.send(msg) {
                Ok(rx) => this.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    this.info = Some((sender, msg));
                    return Poll::Pending;
                }
                Err(SendError::Closed(_)) => {
                    return Poll::Ready(Err(MailboxError::Closed))
                }
            }
        }

        if this.rx.is_some() {
            match unsafe { Pin::new_unchecked(&mut this.rx.as_mut().unwrap()) }.poll(cx)
            {
                Poll::Ready(Ok(i)) => Poll::Ready(Ok(i)),
                Poll::Ready(Err(_)) => Poll::Ready(Err(MailboxError::Closed)),
                Poll::Pending => this.poll_timeout(cx),
            }
        } else {
            Poll::Ready(Err(MailboxError::Closed))
        }
    }
}
