use std::{
    future::Future,
    pin::Pin,
    task::{self, Poll},
    time::Duration,
};

use pin_project_lite::pin_project;
use tokio::sync::oneshot;

use super::{
    channel::{AddressSender, Sender},
    MailboxError, SendError,
};
use crate::{clock::Sleep, handler::Message};

pub type Request<A, M> = MsgRequest<AddressSender<A>, M>;

pub type RecipientRequest<M> = MsgRequest<Box<dyn Sender<M>>, M>;

pin_project! {
    /// A `Future` which represents an asynchronous message sending process.
    #[must_use = "You must wait on the request otherwise the Message will not be delivered"]
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

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

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
                Poll::Ready(res) => Poll::Ready(res.map_err(|_| MailboxError::Closed)),
                Poll::Pending => match this.timeout.as_pin_mut() {
                    Some(timeout) => timeout.poll(cx).map(|_| Err(MailboxError::Timeout)),
                    None => Poll::Pending,
                },
            },
            None => Poll::Ready(Err(MailboxError::Closed)),
        }
    }
}
