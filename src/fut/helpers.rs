use futures::{Async, Future, Poll, Stream};

/// Helper trait that add helper method `finish()` to stream objects.
#[doc(hidden)]
pub trait FinishStream: Sized {
    fn finish(self) -> Finish<Self>;
}

impl<S: Stream> FinishStream for S {
    /// A combinator used to convert stream into a future, future resolves
    /// when stream completes.
    fn finish(self) -> Finish<S> {
        Finish::new(self)
    }
}

/// A combinator used to convert stream into a future, future resolves
/// when stream completes.
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
    type Item = ();
    type Error = S::Error;

    fn poll(&mut self) -> Poll<(), S::Error> {
        loop {
            match self.0.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
                Ok(Async::Ready(Some(_))) => (),
                Err(err) => return Err(err),
            };
        }
    }
}
