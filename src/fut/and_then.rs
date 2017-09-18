use futures::Poll;

use super::chain::Chain;
use super::{CtxFuture, IntoCtxFuture};


/// Future for the `and_then` combinator, chaining a computation onto the end of
/// another future which completes successfully.
///
/// This is created by the `Future::and_then` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct AndThen<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Context=A::Context, Service=A::Service>
{
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F>(future: A, f: F) -> AndThen<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Context=A::Context, Service=A::Service>
{
    AndThen {
        state: Chain::new(future, f),
    }
}

impl<A, B, F> CtxFuture for AndThen<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Context=A::Context, Service=A::Service, Error=A::Error>,
          F: FnOnce(A::Item, &mut A::Context, &mut A::Service) -> B,
{
    type Item = B::Item;
    type Error = B::Error;
    type Context = A::Context;
    type Service = A::Service;

    fn poll(&mut self, ctx: &mut A::Context, srv: &mut A::Service) -> Poll<B::Item, B::Error> {
        self.state.poll(ctx, srv, |result, f, ctx, srv| {
            result.map(|e| {
                Err(f(e, ctx, srv).into_future())
            })
        })
    }
}
