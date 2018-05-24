use futures::{Async, Poll};

use actor::Actor;
use fut::{ActorFuture, ActorStream, IntoActorFuture};

/// A stream combinator which chains a computation onto values produced by a
/// stream.
///
/// This structure is produced by the `ActorStream::and_then` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamAndThen<S, F, U>
where
    U: IntoActorFuture,
{
    stream: S,
    future: Option<U::Future>,
    f: F,
}

pub fn new<S, F, U>(stream: S, f: F) -> StreamAndThen<S, F, U>
where
    S: ActorStream,
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
    U: IntoActorFuture<Error = S::Error>,
{
    StreamAndThen {
        stream,
        f,
        future: None,
    }
}

impl<S, F, U> ActorStream for StreamAndThen<S, F, U>
where
    S: ActorStream,
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
    U: IntoActorFuture<Actor = S::Actor, Error = S::Error>,
{
    type Item = U::Item;
    type Error = S::Error;
    type Actor = S::Actor;

    fn poll(
        &mut self, act: &mut S::Actor, ctx: &mut <S::Actor as Actor>::Context,
    ) -> Poll<Option<U::Item>, S::Error> {
        if self.future.is_none() {
            let item = match try_ready!(self.stream.poll(act, ctx)) {
                None => return Ok(Async::Ready(None)),
                Some(e) => e,
            };
            self.future = Some((self.f)(item, act, ctx).into_future());
        }
        assert!(self.future.is_some());
        match self.future.as_mut().unwrap().poll(act, ctx) {
            Ok(Async::Ready(e)) => {
                self.future = None;
                Ok(Async::Ready(Some(e)))
            }
            Err(e) => {
                self.future = None;
                Err(e)
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
        }
    }
}
