use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::stream::Stream;
use pin_project_lite::pin_project;

/// Helper trait that adds the helper method `finish()` to stream objects.
#[doc(hidden)]
pub trait FinishStream: Sized {
    fn finish(self) -> Finish<Self>;
}

impl<S: Stream> FinishStream for S {
    /// A combinator used to convert a stream into a future; the
    /// future resolves when the stream completes.
    fn finish(self) -> Finish<S> {
        Finish::new(self)
    }
}

pin_project! {
    /// A combinator used to convert a stream into a future; the future
    /// resolves when the stream completes.
    ///
    /// This structure is produced by the `Stream::finish` method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Finish<S> {
        #[pin]
        stream: S
    }
}

impl<S> Finish<S> {
    pub fn new(stream: S) -> Finish<S> {
        Finish { stream }
    }
}

impl<S> Future for Finish<S>
where
    S: Stream,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let this = self.as_mut().project();
            match this.stream.poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Ready(Some(_)) => (),
            };
        }
    }
}
