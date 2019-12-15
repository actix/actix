use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;

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

/// A combinator used to convert a stream into a future; the future
/// resolves when the stream completes.
///
/// This structure is produced by the `Stream::finish` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Finish<S>(S);

impl<S> Finish<S> {
    pub fn new(s: S) -> Finish<S> {
        Finish(s)
    }
}

impl<S> Future for Finish<S>
where
    S: Stream,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        loop {
            match unsafe { Pin::new_unchecked(&mut this.0) }.poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Ready(Some(_)) => (),
            };
        }
    }
}
