use std::time::Duration;
use std::future::Future;
use std::task::Poll;

use tokio_timer::Delay;

use crate::actor::Actor;
use crate::clock;
use crate::fut::ActorStream;

/// Future for the `timeout` combinator, interrupts computations if it takes
/// more than `timeout`.
///
/// This is created by the `ActorFuture::timeout()` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct StreamTimeout<S>
where
    S: ActorStream,
{
    stream: S,
    //err: S::Error,
    dur: Duration,
    timeout: Option<Delay>,
}
/*
pub fn new<S>(stream: S, timeout: Duration, err: S::Error) -> StreamTimeout<S>
where
    S: ActorStream,
    S::Error: Clone,
{
    StreamTimeout {
        stream,
        err,
        dur: timeout,
        timeout: None,
    }
}

impl<S> ActorStream for StreamTimeout<S>
where
    S: ActorStream,
    S::Error: Clone,
{
    type Item = S::Item;
    type Error = S::Error;
    type Actor = S::Actor;

    fn poll(
        &mut self,
        act: &mut S::Actor,
        ctx: &mut <S::Actor as Actor>::Context,
    ) -> Poll<Option<S::Item>, S::Error> {
        match self.stream.poll(act, ctx) {
            Ok(Poll::Ready(res)) => {
                self.timeout.take();
                return Ok(Poll::Ready(res));
            }
            Ok(Poll::Pending) => (),
            Err(err) => return Err(err),
        }

        if self.timeout.is_none() {
            self.timeout = Some(Delay::new(clock::now() + self.dur));
        }

        // check timeout
        match self.timeout.as_mut().unwrap().poll() {
            Ok(Poll::Ready(())) => (),
            Ok(Poll::Pending) => return Ok(Poll::Pending),
            Err(_) => unreachable!(),
        }
        self.timeout.take();

        Err(self.err.clone())
    }
}
*/