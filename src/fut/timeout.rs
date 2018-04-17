use futures::{Async, Future, Poll};
use std::time::Duration;
use tokio_core::reactor::Timeout as TokioTimeout;

use actor::Actor;
use arbiter::Arbiter;
use fut::ActorFuture;

/// Future for the `timeout` combinator, interrupts computations if it takes
/// more than `timeout`.
///
/// This is created by the `ActorFuture::timeout()` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Timeout<F>
where
    F: ActorFuture,
{
    fut: F,
    err: Option<F::Error>,
    timeout: TokioTimeout,
}

pub fn new<F>(future: F, timeout: Duration, err: F::Error) -> Timeout<F>
where
    F: ActorFuture,
{
    Timeout {
        fut: future,
        err: Some(err),
        timeout: TokioTimeout::new(timeout, Arbiter::handle()).unwrap(),
    }
}

impl<F> ActorFuture for Timeout<F>
where
    F: ActorFuture,
{
    type Item = F::Item;
    type Error = F::Error;
    type Actor = F::Actor;

    fn poll(
        &mut self, act: &mut F::Actor, ctx: &mut <F::Actor as Actor>::Context
    ) -> Poll<F::Item, F::Error> {
        // check timeout
        match self.timeout.poll() {
            Ok(Async::Ready(())) => return Err(self.err.take().unwrap()),
            Ok(Async::NotReady) => (),
            Err(_) => unreachable!(),
        }

        self.fut.poll(act, ctx)
    }
}
