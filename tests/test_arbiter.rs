extern crate futures;
#[macro_use]
extern crate actix;

use actix::prelude::*;
use futures::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Message)]
struct Ping(usize);

struct MyActor(Arc<AtomicUsize>);

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl Handler<Ping> for MyActor {
    type Result = ();

    fn handle(&mut self, _: Ping, _: &mut actix::Context<MyActor>) {
        self.0
            .store(self.0.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
        Arbiter::system().do_send(actix::msgs::SystemExit(0));
    }
}

#[test]
fn test_start_actor() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));

    let act_count = Arc::clone(&count);
    let addr = Arbiter::start(move |_| MyActor(act_count));

    addr.do_send(Ping(1));
    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_start_actor_message() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));

    let act_count = Arc::clone(&count);
    let arbiter = Arbiter::new("test2");

    Arbiter::spawn(
        arbiter
            .send(actix::msgs::StartActor::new(move |_| MyActor(act_count)))
            .then(|res| {
                res.unwrap().do_send(Ping(1));
                Ok(())
            }),
    );

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 1);
}
