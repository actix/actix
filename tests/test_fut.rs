use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use actix::clock::{delay_for, Duration};
use actix::prelude::*;
use futures_util::future::FutureExt;

struct MyActor {
    timeout: Arc<AtomicBool>,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        delay_for(Duration::new(0, 5_000_000))
            .then(|_| async {
                System::current().stop();
            })
            .into_actor(self)
            .timeout(Duration::new(0, 100))
            .map(|e, act, _| {
                if e == Err(()) {
                    act.timeout.store(true, Ordering::Relaxed);
                    System::current().stop();
                }
            })
            .wait(ctx);
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
        let mut s = futures_util::stream::FuturesOrdered::new();
        s.push(delay_for(Duration::new(0, 5_000_000)));
        s.push(delay_for(Duration::new(0, 5_000_000)));

        s.into_actor(self)
            .timeout(Duration::new(0, 1000))
            .then(|res, act, _| {
                // Additional waiting time to test `then` call as well
                Box::pin(
                    async move {
                        delay_for(Duration::from_millis(500)).await;
                        res
                    }
                    .into_actor(act),
                )
            })
            .map(|e, act, _| {
                if let Err(()) = e {
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
