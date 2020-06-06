use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

/// A future used to collect all the results of a stream into one generic type.
///
/// This future is returned by the `ActorStream::fold` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamFold<S, F, Fut, T>
where
    Fut: IntoActorFuture,
{
    #[pin]
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
    Fut: IntoActorFuture<Output = T, Actor = S::Actor>,
{
    StreamFold {
        stream,
        f,
        state: State::Ready(t),
    }
}

impl<S, F, Fut, T> ActorFuture for StreamFold<S, F, Fut, T>
where
    S: ActorStream + Unpin,
    F: FnMut(T, S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> Fut,
    Fut: IntoActorFuture<Output = T, Actor = S::Actor>,
    Fut::Future: ActorFuture + Unpin,
{
    type Output = T;
    type Actor = S::Actor;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<T> {
        let mut this = self.get_mut();
        loop {
            match mem::replace(&mut this.state, State::Empty) {
                State::Empty => panic!("cannot poll Fold twice"),
                State::Ready(state) => {
                    match Pin::new(&mut this.stream).poll_next(act, ctx, task) {
                        Poll::Ready(Some(e)) => {
                            let future = (this.f)(state, e, act, ctx);
                            let future = future.into_future();
                            this.state = State::Processing(future);
                        }
                        Poll::Ready(None) => return Poll::Ready(state),
                        Poll::Pending => {
                            this.state = State::Ready(state);
                            return Poll::Pending;
                        }
                    }
                }
                State::Processing(mut fut) => {
                    match Pin::new(&mut fut).poll(act, ctx, task) {
                        Poll::Ready(state) => this.state = State::Ready(state),
                        Poll::Pending => {
                            this.state = State::Processing(fut);
                            return Poll::Pending;
                        }
                    }
                }
            }
        }
    }
}
