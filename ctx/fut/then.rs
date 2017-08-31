use futures::Poll;

use fut::{CtxFuture, IntoCtxFuture};
use fut::chain::Chain;

/// Future for the `then` combinator, chaining computations on the end of
/// another future regardless of its outcome.
///
/// This is created by the `Future::then` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Then<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Context=A::Context, Service=A::Service>
{
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F>(future: A, f: F) -> Then<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Context=A::Context, Service=A::Service>,
{
    Then {
        state: Chain::new(future, f),
    }
}

impl<A, B, F> CtxFuture for Then<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Context=A::Context, Service=A::Service>,
          F: FnOnce(Result<A::Item, A::Error>, &mut A::Context, &mut A::Service) -> B,
{
    type Item = B::Item;
    type Error = B::Error;
    type Context = A::Context;
    type Service = A::Service;

    fn poll(&mut self, ctx: &mut A::Context, srv: &mut A::Service) -> Poll<B::Item, B::Error> {
        self.state.poll(ctx, srv, |a, f, ctx, srv| {
            Ok(Err(f(a, ctx, srv).into_future()))
        })
    }
}
