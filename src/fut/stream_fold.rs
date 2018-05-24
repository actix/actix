use futures::{Async, Poll};
use std::mem;

use actor::Actor; //{Future, Poll, IntoFuture, Async};
use fut::{ActorFuture, ActorStream, IntoActorFuture};

/// A future used to collect all the results of a stream into one generic type.
///
/// This future is returned by the `ActorStream::fold` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamFold<S, F, Fut, T>
where
    Fut: IntoActorFuture,
{
    stream: S,
    f: F,
    state: State<T, Fut::Future>,
}

#[derive(Debug)]
enum State<T, F>
where
    F: ActorFuture,
{
    /// Placeholder state when doing work
    Empty,

    /// Ready to process the next stream item; current accumulator is the `T`
    Ready(T),

    /// Working on a future the process the previous stream item
    Processing(F),
}

pub fn new<S, F, Fut, T>(stream: S, f: F, t: T) -> StreamFold<S, F, Fut, T>
where
    S: ActorStream,
    F: FnMut(T, S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> Fut,
    Fut: IntoActorFuture<Item = T, Actor = S::Actor>,
    S::Error: From<Fut::Error>,
{
    StreamFold {
        stream,
        f,
        state: State::Ready(t),
    }
}

impl<S, F, Fut, T> ActorFuture for StreamFold<S, F, Fut, T>
where
    S: ActorStream,
    F: FnMut(T, S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> Fut,
    Fut: IntoActorFuture<Item = T, Actor = S::Actor>,
    S::Error: From<Fut::Error>,
{
    type Item = T;
    type Error = S::Error;
    type Actor = S::Actor;

    fn poll(
        &mut self, act: &mut S::Actor, ctx: &mut <S::Actor as Actor>::Context,
    ) -> Poll<T, S::Error> {
        loop {
            match mem::replace(&mut self.state, State::Empty) {
                State::Empty => panic!("cannot poll Fold twice"),
                State::Ready(state) => match self.stream.poll(act, ctx)? {
                    Async::Ready(Some(e)) => {
                        let future = (self.f)(state, e, act, ctx);
                        let future = future.into_future();
                        self.state = State::Processing(future);
                    }
                    Async::Ready(None) => return Ok(Async::Ready(state)),
                    Async::NotReady => {
                        self.state = State::Ready(state);
                        return Ok(Async::NotReady);
                    }
                },
                State::Processing(mut fut) => match fut.poll(act, ctx)? {
                    Async::Ready(state) => self.state = State::Ready(state),
                    Async::NotReady => {
                        self.state = State::Processing(fut);
                        return Ok(Async::NotReady);
                    }
                },
            }
        }
    }
}
