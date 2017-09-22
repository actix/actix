use futures::Poll;

use super::chain::Chain;
use super::{CtxFuture, IntoCtxFuture};
use service::Service;


/// Future for the `and_then` combinator, chaining a computation onto the end of
/// another future which completes successfully.
///
/// This is created by the `Future::and_then` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct AndThen<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Service=A::Service>
{
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F>(future: A, f: F) -> AndThen<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Service=A::Service>
{
    AndThen {
        state: Chain::new(future, f),
    }
}

impl<A, B, F> CtxFuture for AndThen<A, B, F>
    where A: CtxFuture,
          B: IntoCtxFuture<Service=A::Service, Error=A::Error>,
          F: FnOnce(A::Item, &mut A::Service, &mut <A::Service as Service>::Context) -> B,
{
    type Item = B::Item;
    type Error = B::Error;
    type Service = A::Service;

    fn poll(&mut self, srv: &mut A::Service,
            ctx: &mut <A::Service as Service>::Context) -> Poll<B::Item, B::Error>
    {
        self.state.poll(srv, ctx, |result, f, srv, ctx| {
            result.map(|e| {
                Err(f(e, srv, ctx).into_future())
            })
        })
    }
}
