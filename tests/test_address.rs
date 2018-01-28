#[macro_use] extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use futures::{future, Future};
use tokio_core::reactor::Timeout;
use actix::prelude::*;

#[derive(Message)]
struct Ping(usize);

struct MyActor(Arc<AtomicUsize>);

impl Actor for MyActor {
    type Context = actix::Context<Self>;
}

impl actix::Handler<Ping> for MyActor {
    type Result = ();

    fn handle(&mut self, _: Ping, _: &mut Self::Context) {
        self.0.store(self.0.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
    }
}

struct MyActor3;

impl Actor for MyActor3 {
    type Context = Context<Self>;
}

impl actix::Handler<Ping> for MyActor3 {
    type Result = MessageResult<Ping>;

    fn handle(&mut self, _: Ping, _: &mut actix::Context<MyActor3>) -> Self::Result {
        Arbiter::system().send(actix::msgs::SystemExit(0));
        Err(())
    }
}

#[test]
fn test_address() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));

    let addr: Address<_> = MyActor(Arc::clone(&count)).start();
    let addr2 = addr.clone();
    addr.send(Ping(0));

    Arbiter::handle().spawn_fn(move || {
        addr2.send(Ping(1));

        Timeout::new(Duration::new(0, 100), Arbiter::handle()).unwrap()
            .then(move |_| {
                addr2.send(Ping(2));
                Arbiter::system().send(actix::msgs::SystemExit(0));
                future::result(Ok(()))
            })
    });

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 3);
}

#[test]
fn test_sync_address() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));
    let arbiter = Arbiter::new("sync-test");

    let addr: SyncAddress<_> = MyActor(Arc::clone(&count)).start();
    let addr2 = addr.clone();
    let addr3 = addr.clone();
    addr.send(Ping(1));

    arbiter.send(actix::msgs::Execute::new(move || -> Result<(), ()> {
        addr3.send(Ping(2));
        Arbiter::system().send(actix::msgs::SystemExit(0));
        Ok(())
    }));
    
    Arbiter::handle().spawn_fn(move || {
        addr2.send(Ping(3));

        Timeout::new(Duration::new(0, 100), Arbiter::handle()).unwrap()
            .then(move |_| {
                addr2.send(Ping(4));
                future::result(Ok(()))
            })
    });

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 4);
}

#[test]
fn test_error_result() {
    let sys = System::new("test");

    let addr: Address<_> = MyActor3.start();

    Arbiter::handle().spawn_fn(move || {
        addr.call_fut(Ping(0)).then(|res| {
            match res {
                Ok(Err(_)) => (),
                _ => panic!("Should not happen"),
            }
            futures::future::result(Ok(()))
        })
    });

    sys.run();
}
