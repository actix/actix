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
          B: IntoCtxFuture<Service=A::Service, Context=A::Context>
{
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F>(future: A, f: F) -> Then<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Service=A::Service, Context=A::Context>,
{
    Then {
        state: Chain::new(future, f),
    }
}

impl<A, B, F> CtxFuture for Then<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Service=A::Service, Context=A::Context>,
          F: FnOnce(Result<A::Item, A::Error>, &mut A::Service, &mut A::Context) -> B,
{
    type Item = B::Item;
    type Error = B::Error;
    type Service = A::Service;
    type Context = A::Context;

    fn poll(&mut self, srv: &mut A::Service, ctx: &mut A::Context) -> Poll<B::Item, B::Error> {
        self.state.poll(srv, ctx, |a, f, srv, ctx| {
            Ok(Err(f(a, srv, ctx).into_future()))
        })
    }
}
