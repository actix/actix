use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};
use std::time::Duration;

use pin_project_lite::pin_project;
use tokio::sync::oneshot;

use crate::clock::Sleep;
use crate::handler::Message;

use super::channel::{AddressSender, Sender};
use super::{MailboxError, SendError};

pub type Request<A, M> = MsgRequest<AddressSender<A>, M>;

pub type RecipientRequest<M> = MsgRequest<Box<dyn Sender<M>>, M>;

pin_project! {
    /// A `Future` which represents an asynchronous message sending process.
    #[must_use = "You have to wait on request otherwise the Message wont be delivered"]
    pub struct MsgRequest<S, M>
    where
        S: Sender<M>,
        M: Message,
        M: Send,
        M::Result: Send
    {
        rx: Option<oneshot::Receiver<M::Result>>,
        info: Option<(S, M)>,
        #[pin]
        timeout: Option<Sleep>,
    }
}

impl<S, M> MsgRequest<S, M>
where
    S: Sender<M>,
    M: Message + Send,
    M::Result: Send,
{
    pub(crate) fn new(rx: Option<oneshot::Receiver<M::Result>>, info: Option<(S, M)>) -> Self {
        Self {
            rx,
            info,
            timeout: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn rx_is_some(&self) -> bool {
        self.rx.is_some()
    }

    /// Set message delivery timeout
    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(actix_rt::time::sleep(dur));
        self
    }
}

impl<S, M> Future for MsgRequest<S, M>
where
    S: Sender<M>,
    M: Message + Send,
    M::Result: Send,
{
    type Output = Result<M::Result, MailboxError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().project();

        if let Some((sender, msg)) = this.info.take() {
            match sender.send(msg) {
                Ok(rx) => *this.rx = Some(rx),
                Err(SendError::Full(msg)) => {
                    *this.info = Some((sender, msg));
                    return Poll::Pending;
                }
                Err(SendError::Closed(_)) => return Poll::Ready(Err(MailboxError::Closed)),
            }
        }

        match this.rx {
            Some(rx) => match Pin::new(rx).poll(cx) {
                Poll::Ready(Ok(i)) => Poll::Ready(Ok(i)),
                Poll::Ready(Err(_)) => Poll::Ready(Err(MailboxError::Closed)),
                Poll::Pending => match this.timeout.as_pin_mut() {
                    Some(timeout) => timeout.poll(cx).map(|_| Err(MailboxError::Timeout)),
                    None => Poll::Pending,
                },
            },
            None => Poll::Ready(Err(MailboxError::Closed)),
        }
    }
}
