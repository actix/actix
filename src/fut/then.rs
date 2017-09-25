use futures::Poll;

use fut::{ActorFuture, IntoActorFuture};
use fut::chain::Chain;
use context::Context;


/// Future for the `then` combinator, chaining computations on the end of
/// another future regardless of its outcome.
///
/// This is created by the `Future::then` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Then<A, B, F>
    where A: ActorFuture,
          B: IntoActorFuture<Actor=A::Actor>
{
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F>(future: A, f: F) -> Then<A, B, F>
    where A: ActorFuture,
          B: IntoActorFuture<Actor=A::Actor>,
{
    Then {
        state: Chain::new(future, f),
    }
}

impl<A, B, F> ActorFuture for Then<A, B, F>
    where A: ActorFuture,
          B: IntoActorFuture<Actor=A::Actor>,
          F: FnOnce(Result<A::Item, A::Error>, &mut A::Actor, &mut Context<A::Actor>) -> B,
{
    type Item = B::Item;
    type Error = B::Error;
    type Actor = A::Actor;

    fn poll(&mut self, act: &mut A::Actor, ctx: &mut Context<A::Actor>) -> Poll<B::Item, B::Error> {
        self.state.poll(act, ctx, |a, f, act, ctx| {
            Ok(Err(f(a, act, ctx).into_future()))
        })
    }
}
