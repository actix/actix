use futures::{Async, Poll};

use actor::Actor;
use fut::{ActorFuture, ActorStream};

/// A combinator used to convert stream into a future, future resolves
/// when stream completes.
///
/// This structure is produced by the `ActorStream::finish` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamFinish<S>(S);

pub fn new<S>(s: S) -> StreamFinish<S>
where
    S: ActorStream,
{
    StreamFinish(s)
}

impl<S> ActorFuture for StreamFinish<S>
where
    S: ActorStream,
{
    type Item = ();
    type Error = S::Error;
    type Actor = S::Actor;

    fn poll(
        &mut self, act: &mut S::Actor, ctx: &mut <S::Actor as Actor>::Context,
    ) -> Poll<(), S::Error> {
        loop {
            match self.0.poll(act, ctx) {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
                Ok(Async::Ready(Some(_))) => (),
                Err(err) => return Err(err),
            };
        }
    }
}
