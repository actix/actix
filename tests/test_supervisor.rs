extern crate actix;
extern crate futures;
extern crate tokio_core;

use actix::prelude::*;
use futures::{future, Future};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio_core::reactor::Timeout;

struct Die;

impl Message for Die {
    type Result = ();
}

struct MyActor(Arc<AtomicUsize>, Arc<AtomicUsize>, Arc<AtomicUsize>);

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, _: &mut Context<MyActor>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
impl actix::Supervised for MyActor {
    fn restarting(&mut self, _: &mut actix::Context<MyActor>) {
        self.1.fetch_add(1, Ordering::Relaxed);
    }
}

impl actix::Handler<Die> for MyActor {
    type Result = ();

    fn handle(&mut self, _: Die, ctx: &mut actix::Context<MyActor>) {
        self.2.fetch_add(1, Ordering::Relaxed);
        ctx.stop();
    }
}

#[test]
fn test_supervisor_restart() {
    let sys = System::new("test");

    let starts = Arc::new(AtomicUsize::new(0));
    let restarts = Arc::new(AtomicUsize::new(0));
    let messages = Arc::new(AtomicUsize::new(0));
    let starts2 = Arc::clone(&starts);
    let restarts2 = Arc::clone(&restarts);
    let messages2 = Arc::clone(&messages);

    let addr: Addr<Unsync, _> =
        actix::Supervisor::start(move |_| MyActor(starts2, restarts2, messages2));
    addr.do_send(Die);
    addr.do_send(Die);

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 100_000), Arbiter::handle())
            .unwrap()
            .then(|_| {
                Arbiter::system().do_send(actix::msgs::SystemExit(0));
                future::result(Ok(()))
            }),
    );

    sys.run();
    assert_eq!(starts.load(Ordering::Relaxed), 3);
    assert_eq!(restarts.load(Ordering::Relaxed), 2);
    assert_eq!(messages.load(Ordering::Relaxed), 2);
}
