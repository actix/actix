extern crate futures;
extern crate actix;
extern crate tokio;

use actix::prelude::*;
use futures::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Message)]
struct Panic();

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
fn test_start_actor() {
    let count = Arc::new(AtomicUsize::new(0));
    let act_count = Arc::clone(&count);

    System::run(move || {
        let addr = Arbiter::start(move |_| MyActor(act_count));

        addr.do_send(Ping(1));
    });

    assert_eq!(count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_start_actor_builder() {
    let count = Arc::new(AtomicUsize::new(0));
    let act_count = Arc::clone(&count);

    System::run(move || {
        let addr = Arbiter::builder().start(move |_| MyActor(act_count));

        addr.do_send(Ping(1));
    });

    assert_eq!(count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_panic_stops_system() {
    let count = Arc::new(AtomicUsize::new(0));
    let act_count = Arc::clone(&count);

    let return_code = System::run(move || {
        let addr = Arbiter::builder()
            .stop_system_on_panic(true)
            .start(move |_| MyActor(act_count));

        addr.do_send(Panic());
    });

    assert_eq!(return_code, 1);
}

#[test]
fn test_start_actor_message() {
    let count = Arc::new(AtomicUsize::new(0));
    let act_count = Arc::clone(&count);

    System::run(move || {
        let arbiter = Arbiter::new("test2");

        tokio::spawn(
            arbiter
                .send(actix::msgs::StartActor::new(move |_| MyActor(act_count)))
                .then(|res| {
                    res.unwrap().do_send(Ping(1));
                    Ok(())
                }),
        );
    });

    assert_eq!(count.load(Ordering::Relaxed), 1);
}
