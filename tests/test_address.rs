#[macro_use]
extern crate actix;
extern crate futures;
extern crate tokio;
extern crate tokio_timer;

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use actix::prelude::*;
use futures::Future;
use tokio_timer::Delay;

#[derive(Message, Debug)]
struct Ping(usize);

struct MyActor(Arc<AtomicUsize>);

impl Actor for MyActor {
    type Context = actix::Context<Self>;
}

impl actix::Handler<Ping> for MyActor {
    type Result = ();

    fn handle(&mut self, _: Ping, _: &mut Self::Context) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

struct MyActor3;

impl Actor for MyActor3 {
    type Context = Context<Self>;
}

impl actix::Handler<Ping> for MyActor3 {
    type Result = ();

    fn handle(&mut self, _: Ping, _: &mut actix::Context<MyActor3>) -> Self::Result {
        System::current().stop();
    }
}

#[test]
fn test_address() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    System::run(move || {
        let arbiter = Arbiter::new("sync-test");

        let addr = MyActor(count2).start();
        let addr2 = addr.clone();
        let addr3 = addr.clone();
        addr.do_send(Ping(1));

        arbiter.do_send(actix::msgs::Execute::new(move || -> Result<(), ()> {
            addr3.do_send(Ping(2));
            Ok(())
        }));

        tokio::spawn(futures::lazy(move || {
            addr2.do_send(Ping(3));

            Delay::new(Instant::now() + Duration::new(0, 100)).then(move |_| {
                addr2.do_send(Ping(4));

                tokio::spawn(Delay::new(Instant::now() + Duration::new(0, 2000)).then(
                    move |_| {
                        System::current().stop();
                        Ok(())
                    },
                ));
                Ok(())
            })
        }));
    });
    thread::sleep(Duration::from_millis(400));

    assert_eq!(count.load(Ordering::Relaxed), 4);
}

#[test]
fn test_sync_recipient_call() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    System::run(move || {
        let addr = MyActor(count2).start();
        let addr2 = addr.clone().recipient();
        addr.do_send(Ping(0));

        tokio::spawn(addr2.send(Ping(1)).then(move |_| {
            addr2.send(Ping(2)).then(|_| {
                System::current().stop();
                Ok(())
            })
        }));
    });

    assert_eq!(count.load(Ordering::Relaxed), 3);
}

#[test]
fn test_error_result() {
    System::run(|| {
        let addr = MyActor3.start();

        tokio::spawn(addr.send(Ping(0)).then(|res| {
            match res {
                Ok(_) => (),
                _ => panic!("Should not happen"),
            }
            Ok(())
        }));
    });
}

struct TimeoutActor;

impl Actor for TimeoutActor {
    type Context = actix::Context<Self>;
}

impl Handler<Ping> for TimeoutActor {
    type Result = ();

    fn handle(&mut self, _: Ping, ctx: &mut Self::Context) {
        Delay::new(Instant::now() + Duration::new(0, 5_000_000))
            .map_err(|_| ())
            .into_actor(self)
            .wait(ctx);
    }
}

#[test]
fn test_message_timeout() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    System::run(move || {
        let addr = TimeoutActor.start();

        addr.do_send(Ping(0));
        tokio::spawn(addr.send(Ping(0)).timeout(Duration::new(0, 1_000)).then(
            move |res| {
                match res {
                    Ok(_) => panic!("Should not happen"),
                    Err(MailboxError::Timeout) => {
                        count2.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => panic!("Should not happen"),
                }
                System::current().stop();
                futures::future::result(Ok(()))
            },
        ));
    });

    assert_eq!(count.load(Ordering::Relaxed), 1);
}

struct TimeoutActor3(Addr<TimeoutActor>, Arc<AtomicUsize>);

impl Actor for TimeoutActor3 {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.0.do_send(Ping(0));
        self.0
            .send(Ping(0))
            .timeout(Duration::new(0, 1_000))
            .into_actor(self)
            .then(move |res, act, _| {
                match res {
                    Ok(_) => panic!("Should not happen"),
                    Err(MailboxError::Timeout) => {
                        act.1.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => panic!("Should not happen"),
                }
                System::current().stop();
                actix::fut::ok(())
            })
            .wait(ctx)
    }
}

#[test]
fn test_call_message_timeout() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    System::run(move || {
        let addr = TimeoutActor.start();
        let _addr2 = TimeoutActor3(addr, count2).start();
    });
    assert_eq!(count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_address_eq() {
    let count0 = Arc::new(AtomicUsize::new(0));
    let count1 = Arc::clone(&count0);

    System::run(move || {
        let addr0 = MyActor(count0).start();
        let addr01 = addr0.clone();
        let addr02 = addr01.clone();

        assert!(addr0 == addr01);
        assert!(addr0 == addr02);

        let addr1 = MyActor(count1).start();

        assert!(addr0 != addr1);

        System::current().stop();
    });
}

#[test]
fn test_address_hash() {
    let count0 = Arc::new(AtomicUsize::new(0));
    let count1 = Arc::clone(&count0);

    System::run(move || {
        let addr0 = MyActor(count0).start();
        let addr01 = addr0.clone();

        let mut addresses = HashSet::new();
        addresses.insert(addr0.clone());
        addresses.insert(addr01.clone());

        assert_eq!(addresses.len(), 1);
        assert!(addresses.contains(&addr0));
        assert!(addresses.contains(&addr01));

        let addr1 = MyActor(count1).start();
        addresses.insert(addr1.clone());

        assert_eq!(addresses.len(), 2);
        assert!(addresses.contains(&addr1));

        assert!(addresses.remove(&addr0));
        assert!(!addresses.contains(&addr0));
        assert!(!addresses.contains(&addr01));
        assert_eq!(addresses.len(), 1);
        assert!(addresses.contains(&addr1));

        System::current().stop();
    });
}

#[test]
fn test_recipient_eq() {
    let count0 = Arc::new(AtomicUsize::new(0));
    let count1 = Arc::clone(&count0);

    System::run(move || {
        let addr0 = MyActor(count0).start();
        let recipient01 = addr0.clone().recipient::<Ping>();
        let recipient02 = addr0.clone().recipient::<Ping>();

        assert!(recipient01 == recipient02);

        let recipient03 = recipient01.clone();
        assert!(recipient01 == recipient03);

        let addr1 = MyActor(count1).start();
        let recipient11 = addr1.clone().recipient::<Ping>();

        assert!(recipient01 != recipient11);

        System::current().stop();
    });
}

#[test]
fn test_recipient_hash() {
    let count0 = Arc::new(AtomicUsize::new(0));
    let count1 = Arc::clone(&count0);

    System::run(move || {
        let addr0 = MyActor(count0).start();
        let recipient01 = addr0.clone().recipient::<Ping>();
        let recipient02 = addr0.clone().recipient::<Ping>();

        let mut recipients = HashSet::new();
        recipients.insert(recipient01.clone());
        recipients.insert(recipient02.clone());

        assert_eq!(recipients.len(), 1);
        assert!(recipients.contains(&recipient01));
        assert!(recipients.contains(&recipient02));

        let addr1 = MyActor(count1).start();
        let recipient11 = addr1.clone().recipient::<Ping>();
        recipients.insert(recipient11.clone());

        assert_eq!(recipients.len(), 2);
        assert!(recipients.contains(&recipient11));

        assert!(recipients.remove(&recipient01));
        assert!(!recipients.contains(&recipient01));
        assert!(!recipients.contains(&recipient02));
        assert_eq!(recipients.len(), 1);
        assert!(recipients.contains(&recipient11));

        System::current().stop();
    });
}
