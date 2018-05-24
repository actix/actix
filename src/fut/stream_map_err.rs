use futures::Poll;

use actor::Actor;
use fut::ActorStream;

/// A stream combinator which will change the error type of a stream from one
/// type to another.
///
/// This is produced by the `ActorStream::map_err` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamMapErr<S, F> {
    stream: S,
    f: F,
}

pub fn new<S, F, U>(stream: S, f: F) -> StreamMapErr<S, F>
where
    S: ActorStream,
    F: FnMut(S::Error, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
{
    StreamMapErr { stream, f }
}

impl<S, F, U> ActorStream for StreamMapErr<S, F>
where
    S: ActorStream,
    F: FnMut(S::Error, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
{
    type Item = S::Item;
    type Error = U;
    type Actor = S::Actor;

    fn poll(
        &mut self, act: &mut Self::Actor, ctx: &mut <S::Actor as Actor>::Context,
    ) -> Poll<Option<S::Item>, U> {
        match self.stream.poll(act, ctx) {
            Ok(ok) => Ok(ok),
            Err(e) => Err((self.f)(e, act, ctx)),
        }
    }
}
