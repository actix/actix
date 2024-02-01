use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use actix::prelude::*;
use tokio::sync::oneshot;

#[derive(Debug)]
struct Ping;

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

#[test]
fn test_start_actor_message() {
    let count = Arc::new(AtomicUsize::new(0));
    let act_count = Arc::clone(&count);

    let sys = System::new();

    sys.block_on(async move {
        let arbiter = Arbiter::new();

        actix_rt::spawn(async move {
            let (tx, rx) = oneshot::channel();

            arbiter.spawn_fn(move || {
                let addr = MyActor(act_count).start();
                tx.send(addr).ok().unwrap();
            });

            // TODO: investigate under CPU stress and/or with a drop impl
            // original test used this line, but was buggy:
            // rx.await.unwrap().do_send(Ping(1));

            rx.await.unwrap().send(Ping).await.unwrap();
        });
    });

    sys.run().unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 1);
}
