use futures::{Async, Poll};

use actor::Actor;
use fut::ActorStream;

/// A stream combinator which will change the type of a stream from one
/// type to another.
///
/// This is produced by the `ActorStream::map` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamMap<S, F> {
    stream: S,
    f: F,
}

pub fn new<S, F, U>(stream: S, f: F) -> StreamMap<S, F>
where
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
    S: ActorStream,
{
    StreamMap { stream, f }
}

impl<S, F, U> ActorStream for StreamMap<S, F>
where
    S: ActorStream,
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
{
    type Item = U;
    type Error = S::Error;
    type Actor = S::Actor;

    fn poll(
        &mut self, act: &mut Self::Actor, ctx: &mut <S::Actor as Actor>::Context,
    ) -> Poll<Option<U>, S::Error> {
        match self.stream.poll(act, ctx) {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(option)) => {
                if let Some(item) = option {
                    Ok(Async::Ready(Some((self.f)(item, act, ctx))))
                } else {
                    Ok(Async::Ready(None))
                }
            }
            Err(e) => Err(e),
        }
    }
}
