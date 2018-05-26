extern crate actix;
extern crate futures;
extern crate tokio_timer;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix::msgs::SystemExit;
use actix::prelude::*;
use futures::stream::futures_ordered;
use futures::{Future, Stream};
use tokio_timer::Delay;

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
        Delay::new(Instant::now() + Duration::new(0, 5_000_000))
            .then(|_| {
                Arbiter::system().do_send(SystemExit(0));
                Ok::<_, Error>(())
            })
            .into_actor(self)
            .timeout(Duration::new(0, 100), Error::Timeout)
            .map_err(|e, act, _| {
                if e == Error::Timeout {
                    act.timeout.store(true, Ordering::Relaxed);
                    Arbiter::system().do_send(SystemExit(0));
                    ()
                }
            })
            .wait(ctx)
    }
}

#[test]
fn test_fut_timeout() {
    let sys = System::new("test");
    let timeout = Arc::new(AtomicBool::new(false));

    let _addr = MyActor {
        timeout: Arc::clone(&timeout),
    }.start();

    sys.run();
    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}

struct MyStreamActor {
    timeout: Arc<AtomicBool>,
}

impl Actor for MyStreamActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let s = futures_ordered(vec![
            Delay::new(Instant::now() + Duration::new(0, 5_000_000)),
            Delay::new(Instant::now() + Duration::new(0, 5_000_000)),
        ]);

        s.map_err(|_| Error::Generic)
            .into_actor(self)
            .timeout(Duration::new(0, 1000), Error::Timeout)
            .map_err(|e, act, _| {
                if e == Error::Timeout {
                    act.timeout.store(true, Ordering::Relaxed);
                    Arbiter::system().do_send(SystemExit(0));
                }
            })
            .finish()
            .wait(ctx)
    }
}

#[test]
fn test_stream_timeout() {
    let sys = System::new("test");
    let timeout = Arc::new(AtomicBool::new(false));

    let _addr = MyStreamActor {
        timeout: Arc::clone(&timeout),
    }.start();

    sys.run();
    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}
