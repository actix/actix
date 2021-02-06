use std::mem::drop;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::{collections::HashSet, time::Duration};

use actix::prelude::*;
use actix_rt::time::sleep;

#[derive(Debug)]
struct Ping(usize);

impl Message for Ping {
    type Result = ();
}

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

#[derive(Debug)]
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

    let sys = System::new();
    sys.block_on(async move {
        let arbiter = Arbiter::new();

        let addr = MyActor(count2).start();
        let addr2 = addr.clone();
        let addr3 = addr.clone();
        addr.do_send(Ping(1));

        arbiter.spawn_fn(move || {
            addr3.do_send(Ping(2));

            actix_rt::spawn(async move {
                let _ = addr2.send(Ping(3)).await;
                let _ = addr2.send(Ping(4)).await;
                System::current().stop();
            });
        });
    });

    sys.run().unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 4);
}

struct WeakAddressRunner;

impl Actor for WeakAddressRunner {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr1 = MyActor3.start();
        let addr2 = MyActor3.start();
        let weak1 = addr1.downgrade();
        let weak2 = addr2.downgrade();
        drop(addr1);

        ctx.run_later(Duration::new(0, 1000), move |_, _| {
            assert!(
                weak1.upgrade().is_none(),
                "Should not be able to upgrade weak1!"
            );
            match weak2.upgrade() {
                Some(addr) => {
                    assert!(addr2 == addr);
                }
                None => panic!("Should be able to upgrade weak2!"),
            }
            System::current().stop();
        });
    }
}

#[test]
fn test_weak_address() {
    System::new().block_on(async move {
        WeakAddressRunner.start();
    });
}

struct WeakRecipientRunner;

impl Actor for WeakRecipientRunner {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let count0 = Arc::new(AtomicUsize::new(0));
        let addr1 = MyActor(count0).start();
        let addr2 = MyActor3.start();

        // let recipient1 = addr1.clone().recipient();
        let recipient2 = addr2.clone().recipient();

        let weak1 = addr1.downgrade().recipient();
        let weak2 = addr2.downgrade().recipient();
        // not strictly necessary, weak1 is dropped by RIAA since nobody in `run_later` uses it.
        // to make this test red, move drop(addr1) to the line before `System::current().stop()`
        drop(addr1);

        ctx.run_later(Duration::new(0, 1000), move |_, _| {
            if weak1.upgrade().is_some() {
                System::current().stop_with_code(1);
                panic!("Should not be able to upgrade weak1!");
            }
            match weak2.upgrade() {
                Some(addr) => {
                    assert!(recipient2 == addr);
                }
                None => panic!("Should be able to upgrade weak2!"),
            }
            System::current().stop();
        });
    }
}

#[test]
fn test_weak_recipient() {
    System::new().block_on(async move {
        WeakRecipientRunner.start();
    });
}

#[test]
fn test_sync_recipient_call() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    let sys = System::new();
    sys.block_on(async move {
        let addr = MyActor(count2).start();
        let addr2 = addr.clone().recipient();
        addr.do_send(Ping(0));

        actix_rt::spawn(async move {
            let _ = addr2.send(Ping(1)).await;
            let _ = addr2.send(Ping(2)).await;
            System::current().stop();
        });
    });

    sys.run().unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 3);
}

#[test]
fn test_error_result() {
    System::new().block_on(async {
        let addr = MyActor3.start();

        actix_rt::spawn(async move {
            let res = addr.send(Ping(0)).await;
            match res {
                Ok(_) => (),
                _ => panic!("Should not happen"),
            };
        });
    });
}

struct TimeoutActor;

impl Actor for TimeoutActor {
    type Context = actix::Context<Self>;
}

impl Handler<Ping> for TimeoutActor {
    type Result = ();

    fn handle(&mut self, _: Ping, ctx: &mut Self::Context) {
        sleep(Duration::new(0, 5_000_000))
            .into_actor(self)
            .wait(ctx);
    }
}

#[test]
fn test_message_timeout() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    let sys = System::new();
    sys.block_on(async move {
        let addr = TimeoutActor.start();

        addr.do_send(Ping(0));
        actix_rt::spawn(async move {
            let res = addr.send(Ping(0)).timeout(Duration::new(0, 1_000)).await;
            match res {
                Ok(_) => panic!("Should not happen"),
                Err(MailboxError::Timeout) => {
                    count2.fetch_add(1, Ordering::Relaxed);
                }
                _ => panic!("Should not happen"),
            }
            System::current().stop();
        });
    });

    sys.run().unwrap();

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
                async {}.into_actor(act)
            })
            .wait(ctx);
    }
}

#[test]
fn test_call_message_timeout() {
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::clone(&count);

    let sys = System::new();
    sys.block_on(async move {
        let addr = TimeoutActor.start();
        let _addr2 = TimeoutActor3(addr, count2).start();
    });

    sys.run().unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_address_eq() {
    let count0 = Arc::new(AtomicUsize::new(0));
    let count1 = Arc::clone(&count0);

    System::new().block_on(async move {
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

    System::new().block_on(async move {
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

    System::new().block_on(async move {
        let addr0 = MyActor(count0).start();
        let recipient01 = addr0.clone().recipient::<Ping>();
        let recipient02 = addr0.recipient::<Ping>();

        assert!(recipient01 == recipient02);

        let recipient03 = recipient01.clone();
        assert!(recipient01 == recipient03);

        let addr1 = MyActor(count1).start();
        let recipient11 = addr1.recipient::<Ping>();

        assert!(recipient01 != recipient11);

        System::current().stop();
    });
}

#[test]
fn test_recipient_hash() {
    let count0 = Arc::new(AtomicUsize::new(0));
    let count1 = Arc::clone(&count0);

    System::new().block_on(async move {
        let addr0 = MyActor(count0).start();
        let recipient01 = addr0.clone().recipient::<Ping>();
        let recipient02 = addr0.recipient::<Ping>();

        let mut recipients = HashSet::new();
        recipients.insert(recipient01.clone());
        recipients.insert(recipient02.clone());

        assert_eq!(recipients.len(), 1);
        assert!(recipients.contains(&recipient01));
        assert!(recipients.contains(&recipient02));

        let addr1 = MyActor(count1).start();
        let recipient11 = addr1.recipient::<Ping>();
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
