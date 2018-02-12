extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use futures::{Future, Stream};
use futures::stream::futures_ordered;
use tokio_core::reactor::Timeout;
use actix::prelude::*;
use actix::msgs::SystemExit;

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
        Timeout::new(Duration::new(0, 5_000_000), Arbiter::handle()).unwrap()
            .then(|_| {
                Arbiter::system().send(SystemExit(0));
                Ok::<_, Error>(())
            })
            .into_actor(self)
            .timeout(Duration::new(0, 100), Error::Timeout)
            .map_err(|e, act, _| if e == Error::Timeout {
                act.timeout.store(true, Ordering::Relaxed);
                Arbiter::system().send(SystemExit(0));
                ()
            })
            .wait(ctx)
    }
}

#[test]
fn test_fut_timeout() {
    let sys = System::new("test");
    let timeout = Arc::new(AtomicBool::new(false));

    let _addr: Addr<Unsync, _> = MyActor {timeout: Arc::clone(&timeout)}.start();

    sys.run();
    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}


struct MyStreamActor {
    timeout: Arc<AtomicBool>,
}

impl Actor for MyStreamActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let s = futures_ordered(
            vec![Timeout::new(Duration::new(0, 5_000_000), Arbiter::handle()),
                 Timeout::new(Duration::new(0, 5_000_000), Arbiter::handle())]);

        s.and_then(|f| f)
            .map_err(|_| Error::Generic)
            .into_actor(self)
            .timeout(Duration::new(0, 100), Error::Timeout)
            .map_err(|e, act, _| if e == Error::Timeout {
                act.timeout.store(true, Ordering::Relaxed);
                Arbiter::system().send(SystemExit(0));
                ()
            })
            .finish()
            .wait(ctx)
    }
}

#[test]
fn test_stream_timeout() {
    let sys = System::new("test");
    let timeout = Arc::new(AtomicBool::new(false));

    let _addr: Addr<Unsync, _> = MyStreamActor {timeout: Arc::clone(&timeout)}.start();

    sys.run();
    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}
