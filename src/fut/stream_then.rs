use futures::Future;
use std::task::Poll;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

/// A stream combinator which chains a computation onto each item produced by a
/// stream.
///
/// This structure is produced by the `ActorStream::then` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamThen<S, F, U>
where
    U: IntoActorFuture,
{
    stream: S,
    future: Option<U::Future>,
    f: F,
}
/*
pub fn new<S, F, U>(stream: S, f: F) -> StreamThen<S, F, U>
where
    S: ActorStream,
    F: FnMut(
        Result<S::Item, S::Error>,
        &mut S::Actor,
        &mut <S::Actor as Actor>::Context,
    ) -> U,
    U: IntoActorFuture<Actor = S::Actor>,
{
    StreamThen {
        stream,
        f,
        future: None,
    }
}
impl<S, F, U> ActorStream for StreamThen<S, F, U>
where
    S: ActorStream,
    F: FnMut(
        Result<S::Item, S::Error>,
        &mut S::Actor,
        &mut <S::Actor as Actor>::Context,
    ) -> U,
    U: IntoActorFuture<Actor = S::Actor>,
{
    type Item = U::Item;
    type Error = U::Error;
    type Actor = S::Actor;
    fn poll(
        &mut self,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
    ) -> Poll<Option<U::Item>, U::Error> {
        if self.future.is_none() {
            let item = match self.stream.poll(act, ctx) {
                Ok(Poll::Pending) => return Ok(Poll::Pending),
                Ok(Poll::Ready(None)) => return Ok(Poll::Ready(None)),
                Ok(Poll::Ready(Some(e))) => Ok(e),
                Err(e) => Err(e),
            };
            self.future = Some((self.f)(item, act, ctx).into_future());
        }
        assert!(self.future.is_some());
        match self.future.as_mut().unwrap().poll(act, ctx) {
            Ok(Poll::Ready(e)) => {
                self.future = None;
                Ok(Poll::Ready(Some(e)))
            }
            Err(e) => {
                self.future = None;
                Err(e)
            }
            Ok(Poll::Pending) => Ok(Poll::Pending),
        }
    }
}
*/
