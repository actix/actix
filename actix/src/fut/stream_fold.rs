use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::{ActorFuture, ActorStream, IntoActorFuture};

pin_project! {
    /// A future used to collect all the results of a stream into one generic type.
    ///
    /// This future is returned by the `ActorStream::fold` method.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct StreamFold<S, F, Fut, T> {
        #[pin]
        stream: S,
        f: F,
        #[pin]
        state: State<T, Fut>,
    }
}

pin_project! {
    #[project = FoldStateProj]
    #[derive(Debug)]
    enum State<T, F> {
        /// Placeholder state when doing work
        Empty,

        /// Ready to process the next stream item; current accumulator is the `T`
        Ready {
            res: Option<T>
        },

        /// Working on a future the process the previous stream item
        Processing {
            #[pin]
            fut: F
        },
    }
}

pub fn new<S, A, F, Fut, T>(stream: S, f: F, t: T) -> StreamFold<S, F, Fut::Future, T>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(T, S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: IntoActorFuture<A, Output = T>,
{
    StreamFold {
        stream,
        f,
        state: State::Ready { res: Some(t) },
    }
}

impl<S, A, F, Fut, T> ActorFuture<A> for StreamFold<S, F, Fut::Future, T>
where
    S: ActorStream<A>,
    A: Actor,
    F: FnMut(T, S::Item, &mut A, &mut A::Context) -> Fut,
    Fut: IntoActorFuture<A, Output = T>,
    Fut::Future: ActorFuture<A>,
{
    type Output = T;

    fn poll(
        mut self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<T> {
        loop {
            let this = self.as_mut().project();
            match this.state.project() {
                FoldStateProj::Ready { res } => match this.stream.poll_next(act, ctx, task) {
                    Poll::Ready(Some(e)) => {
                        let future = (this.f)(res.take().unwrap(), e, act, ctx);
                        let fut = future.into_future();
                        self.as_mut().project().state.set(State::Processing { fut });
                    }
                    Poll::Ready(None) => {
                        return {
                            let res = res.take().unwrap();
                            self.project().state.set(State::Empty);
                            Poll::Ready(res)
                        }
                    }
                    Poll::Pending => return Poll::Pending,
                },
                FoldStateProj::Processing { fut } => match fut.poll(act, ctx, task) {
                    Poll::Ready(state) => self
                        .as_mut()
                        .project()
                        .state
                        .set(State::Ready { res: Some(state) }),
                    Poll::Pending => return Poll::Pending,
                },
                FoldStateProj::Empty => panic!("cannot poll Fold twice"),
            }
        }
    }
}
