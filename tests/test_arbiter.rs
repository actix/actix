extern crate actix;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use actix::prelude::*;

#[derive(Debug)]
struct Ping(usize);

impl ResponseType for Ping {
    type Item = ();
    type Error = ();
}

struct MyActor(Arc<AtomicUsize>);

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl Handler<Ping> for MyActor {

    fn handle(&mut self, _: Ping, _: &mut Context<MyActor>) -> Response<Self, Ping> {
        self.0.store(self.0.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
        Arbiter::system().send(msgs::SystemExit(0));
        Self::empty()
    }
}

#[test]
fn test_start_actor() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));

    let act_count = Arc::clone(&count);
    let addr = Arbiter::start(move |_| {
        MyActor(act_count)
    });

    addr.send(Ping(1));
    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 1);
}
