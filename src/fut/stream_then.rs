use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

/// A stream combinator which chains a computation onto each item produced by a
/// stream.
///
/// This structure is produced by the `ActorStream::then` method.
#[pin_project]
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamThen<S, F: 'static, U>
where
    U: IntoActorFuture,
    S: ActorStream,
{
    #[pin]
    stream: S,
    #[pin]
    future: FutureState<U::Future>,
    f: F,
}

#[pin_project(project = FutureProj)]
#[derive(Debug)]
enum FutureState<F> {
    Some(#[pin] F),
    None,
}

pub fn new<S, F: 'static, U>(stream: S, f: F) -> StreamThen<S, F, U>
where
    S: ActorStream,
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
    U: IntoActorFuture<Actor = S::Actor>,
{
    StreamThen {
        stream,
        f,
        future: FutureState::None,
    }
}

impl<S, F: 'static, U> ActorStream for StreamThen<S, F, U>
where
    S: ActorStream,
    F: FnMut(S::Item, &mut S::Actor, &mut <S::Actor as Actor>::Context) -> U,
    U: IntoActorFuture<Actor = S::Actor>,
{
    type Item = U::Output;
    type Actor = S::Actor;

    fn poll_next(
        mut self: Pin<&mut Self>,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<U::Output>> {
        loop {
            let this = self.as_mut().project();
            match this.future.project() {
                FutureProj::None => {
                    let item = match this.stream.poll_next(act, ctx, task) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(None) => return Poll::Ready(None),
                        Poll::Ready(Some(e)) => e,
                    };
                    let fut = (this.f)(item, act, ctx).into_future();
                    let state = FutureState::Some(fut);
                    self.as_mut().project().future.set(state);
                }
                FutureProj::Some(fut) => {
                    return match fut.poll(act, ctx, task) {
                        Poll::Ready(e) => {
                            self.project().future.set(FutureState::None);
                            Poll::Ready(Some(e))
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
            }
        }
        //
        // let this = self.project();
        // if this.future.is_none() {
        //     let item = match this.stream.poll_next(act, ctx, task) {
        //         Poll::Pending => return Poll::Pending,
        //         Poll::Ready(None) => return Poll::Ready(None),
        //         Poll::Ready(Some(e)) => e,
        //     };
        //     *this.future = Some((this.f)(item, act, ctx).into_future());
        // }
        // assert!(this.future.is_some());
        // match Pin::new(this.future.as_mut().unwrap()).poll(act, ctx, task) {
        //     Poll::Ready(e) => {
        //         *this.future = None;
        //         Poll::Ready(Some(e))
        //     }
        //     Poll::Pending => Poll::Pending,
        // }
    }
}
