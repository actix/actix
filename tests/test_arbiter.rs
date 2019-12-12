use actix::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug)]
struct Panic();

impl Message for Panic {
    type Result = ();
}

#[derive(Debug)]
struct Ping(usize);

impl Message for Ping {
    type Result = ();
}

struct MyActor(Arc<AtomicUsize>);

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl Handler<Ping> for MyActor {
    type Result = ();

    fn handle(&mut self, _: Ping, _: &mut actix::Context<MyActor>) {
        self.0
            .store(self.0.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
        System::current().stop();
    }
}

impl Handler<Panic> for MyActor {
    type Result = ();

    fn handle(&mut self, _: Panic, _: &mut actix::Context<MyActor>) {
        panic!("Whoops!");
    }
}

#[test]
fn test_start_actor_message() {
    let count = Arc::new(AtomicUsize::new(0));
    let act_count = Arc::clone(&count);

    System::run(move || {
        let arbiter = Arbiter::new();

        actix_rt::spawn(async move {
            let res = arbiter.exec(|| MyActor(act_count).start()).await;
            res.unwrap().do_send(Ping(1));
        });
    })
    .unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 1);
}
