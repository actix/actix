extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use futures::{future, Future};
use tokio_core::reactor::Timeout;
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
    type Result = ();

    fn handle(&mut self, msg: Ping, _: &mut Context<MyActor>) {
        println!("PING: {:?}", msg);
        self.0.store(self.0.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
    }
}

struct MyActor2(Option<Address<MyActor>>, Option<SyncAddress<MyActor>>);

impl Actor for MyActor2 {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.0.take().unwrap().upgrade()
            .actfuture()
            .then(move |addr, act: &mut Self, _: &mut _| {
                let addr = addr.unwrap();
                addr.send(Ping(10));
                act.1 = Some(addr);
                Arbiter::system().send(msgs::SystemExit(0));
                fut::ok(())
            }).spawn(ctx);
    }
}

struct MyActor3;

impl Actor for MyActor3 {
    type Context = Context<Self>;
}

impl Handler<Ping> for MyActor3 {
    type Result = ResponseResult<Ping>;

    fn handle(&mut self, _: Ping, _: &mut Context<MyActor3>) -> Self::Result {
        Arbiter::system().send(msgs::SystemExit(0));
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
                Arbiter::system().send(msgs::SystemExit(0));
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

    arbiter.send(msgs::Execute::new(move || -> Result<(), ()> {
        addr3.send(Ping(2));
        Arbiter::system().send(msgs::SystemExit(0));
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
fn test_address_upgrade() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));

    let addr: Address<_> = MyActor(Arc::clone(&count)).start();
    addr.send(Ping(0));

    let addr2 = addr.clone();
    let _addr3: Address<_> = MyActor2(Some(addr2), None).start();

    Arbiter::handle().spawn_fn(move || {
        Timeout::new(Duration::new(0, 1000), Arbiter::handle()).unwrap()
            .then(move |_| {
                addr.send(Ping(3));
                Arbiter::handle().spawn_fn(move || {
                    Arbiter::system().send(msgs::SystemExit(0));
                    future::result(Ok(()))
                });
                future::result(Ok(()))
            })
    });

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 3);
}

#[test]
fn test_error_result() {
    let sys = System::new("test");

    let addr: Address<_> = MyActor3.start();

    Arbiter::handle().spawn_fn(move || {
        addr.call_fut(Ping(0), false).then(|res| {
            match res {
                Ok(Err(_)) => (),
                _ => panic!("Should not happen"),
            }
            futures::future::result(Ok(()))
        })
    });

    sys.run();
}
