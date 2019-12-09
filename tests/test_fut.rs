use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use actix::prelude::*;
use futures::{FutureExt, StreamExt};
use tokio::time::{delay_for, Duration, Instant};

struct MyActor {
    timeout: Arc<AtomicBool>,
}

#[derive(PartialEq, Copy, Clone)]
enum Error {
    Timeout,
    Generic,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        delay_for(Duration::new(0, 5_000_000))
            .into_actor(self)
            .timeout(Duration::new(0, 100), Error::Timeout)
            .map(|e, act, _| {
                if e == Err(Error::Timeout) {
                    act.timeout.store(true, Ordering::Relaxed);
                    System::current().stop();
                    ()
                }
            })
            .then(|_, _, _| {
                System::current().stop();
                async { () }
            })
            .wait(ctx)
    }
}

#[test]
fn test_fut_timeout() {
    let timeout = Arc::new(AtomicBool::new(false));
    let timeout2 = Arc::clone(&timeout);

    System::run(move || {
        let _addr = MyActor { timeout: timeout2 }.start();
    })
    .unwrap();

    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}

struct MyStreamActor {
    timeout: Arc<AtomicBool>,
}

impl Actor for MyStreamActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let s = futures::stream::iter(vec![
            delay_for(Duration::new(0, 5_000_000)),
            delay_for(Duration::new(0, 5_000_000)),
        ]);

        s.into_actor(self)
            .timeout(Duration::new(0, 1000), Error::Timeout)
            .map_err(|e, act, _| {
                if e == Error::Timeout {
                    act.timeout.store(true, Ordering::Relaxed);
                    System::current().stop();
                }
            })
            .finish()
            .wait(ctx)
    }
}

#[test]
fn test_stream_timeout() {
    let timeout = Arc::new(AtomicBool::new(false));
    let timeout2 = Arc::clone(&timeout);

    System::run(|| {
        let _addr = MyStreamActor { timeout: timeout2 }.start();
    })
    .unwrap();

    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}
