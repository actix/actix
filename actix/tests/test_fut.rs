use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use actix::clock::sleep;
use actix::prelude::*;

struct MyActor {
    timeout: Arc<AtomicBool>,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        async {
            sleep(Duration::new(0, 5_000_000)).await;
            System::current().stop();
        }
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

    let sys = System::new();
    sys.block_on(async {
        let _addr = MyActor { timeout: timeout2 }.start();
    });

    sys.run().unwrap();

    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}

struct MyStreamActor {
    timeout: Arc<AtomicBool>,
}

impl Actor for MyStreamActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let mut s = futures_util::stream::FuturesOrdered::new();
        s.push(sleep(Duration::new(0, 5_000_000)));
        s.push(sleep(Duration::new(0, 5_000_000)));

        s.into_actor(self)
            .timeout(Duration::new(0, 1000))
            .then(|res, act, _| {
                // Additional waiting time to test `then` call as well
                Box::pin(
                    async move {
                        sleep(Duration::from_millis(500)).await;
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

    let sys = System::new();
    sys.block_on(async {
        let _addr = MyStreamActor { timeout: timeout2 }.start();
    });
    sys.run().unwrap();

    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}
