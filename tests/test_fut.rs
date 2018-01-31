extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use futures::Future;
use tokio_core::reactor::Timeout;
use actix::prelude::*;
use actix::msgs::SystemExit;

struct MyActor {
    timeout: Arc<AtomicBool>,
}

#[derive(PartialEq)]
enum Error {
    Timeout,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        Timeout::new(Duration::new(0, 1_000_000), Arbiter::handle()).unwrap()
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
fn test_active_address() {
    let sys = System::new("test");
    let timeout = Arc::new(AtomicBool::new(false));

    let _addr: Address<_> = MyActor {timeout: Arc::clone(&timeout)}.start();

    sys.run();
    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}
