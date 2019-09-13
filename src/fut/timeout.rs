use std::time::Duration;
use std::future::Future;
use std::task::Poll;

use tokio_timer::Delay;

use crate::actor::Actor;
use crate::clock;
use crate::fut::ActorFuture;

/// Future for the `timeout` combinator, interrupts computations if it takes
/// more than `timeout`.
///
/// This is created by the `ActorFuture::timeout()` method.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct Timeout<I,E,F>
where
    F: ActorFuture<Item=Result<I,E>>,
{
    fut: F,
    err: Option<E>,
    timeout: Delay,
}
/*
pub fn new<F>(future: F, timeout: Duration, err: F::Error) -> Timeout<F>
where
    F: ActorFuture,
{
    Timeout {
        fut: future,
        err: Some(err),
        timeout: Delay::new(clock::now() + timeout),
    }
}

impl<F> ActorFuture for Timeout<F>
where
    F: ActorFuture,
{
    type Item = F::Item;
    type Actor = F::Actor;

    fn poll(
        &mut self,
        act: &mut F::Actor,
        ctx: &mut <F::Actor as Actor>::Context,
    ) -> Poll<F::Item, F::Error> {
        // check timeout
        match self.timeout.poll() {
            Ok(Poll::Ready(())) => return Err(self.err.take().unwrap()),
            Ok(Poll::Pending) => (),
            Err(_) => unreachable!(),
        }

        self.fut.poll(act, ctx)
    }
}
*/