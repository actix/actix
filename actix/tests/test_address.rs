use std::mem::drop;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::{collections::HashSet, time::Duration};

use actix_rt::time::sleep;

use actix::prelude::*;
use actix::WeakRecipient;

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

#[derive(Debug)]
struct PingCounterActor {
    ping_count: Arc<AtomicUsize>,
}

impl Default for PingCounterActor {
    fn default() -> Self {
        Self {
            ping_count: Arc::new(AtomicUsize::from(0)),
        }
    }
}

impl Actor for PingCounterActor {
    type Context = Context<Self>;
}

struct CountPings;

impl Message for CountPings {
    type Result = usize;
}

impl actix::Handler<Ping> for PingCounterActor {
    type Result = ();

    fn handle(&mut self, _msg: Ping, _ctx: &mut Self::Context) -> Self::Result {
        self.ping_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl actix::Handler<CountPings> for PingCounterActor {
    type Result = <CountPings as actix::Message>::Result;

    fn handle(&mut self, _msg: CountPings, _ctx: &mut Self::Context) -> Self::Result {
        self.ping_count.load(Ordering::SeqCst)
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
    let sys = System::new();

    sys.block_on(async move {
        WeakAddressRunner.start();
    });

    sys.run().unwrap();
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
    let sys = System::new();

    sys.block_on(async move {
        WeakRecipientRunner.start();
    });

    sys.run().unwrap();
}

#[test]
fn test_weak_recipient_can_be_cloned() {
    let sys = System::new();

    sys.block_on(async move {
        let addr = PingCounterActor::start_default();
        let weak_recipient = addr.downgrade().recipient();
        let weak_recipient_clone = weak_recipient.clone();

        weak_recipient
            .upgrade()
            .expect("must be able to upgrade the weak recipient here")
            .send(Ping(0))
            .await
            .expect("send must not fail");
        weak_recipient_clone
            .upgrade()
            .expect("must be able to upgrade the cloned weak recipient here")
            .send(Ping(0))
            .await
            .expect("send must not fail");
        let pings = addr.send(CountPings {}).await.expect("send must not fail");
        assert_eq!(
            pings, 2,
            "both the weak recipient and its clone must have sent a ping"
        );
    });
}

#[test]
fn test_recipient_can_be_downgraded() {
    let sys = System::new();

    sys.block_on(async move {
        let addr = PingCounterActor::start_default();
        let strong_recipient: Recipient<Ping> = addr.clone().recipient();
        // test the downgrade method
        let weak_recipient: WeakRecipient<Ping> = strong_recipient.downgrade();
        // test the From trait
        let converted_weak_recipient = WeakRecipient::from(strong_recipient);
        weak_recipient
            .upgrade()
            .expect("upgrade of weak recipient must not fail here")
            .send(Ping(0))
            .await
            .unwrap();

        converted_weak_recipient
            .upgrade()
            .expect("upgrade of weak recipient must not fail here")
            .send(Ping(0))
            .await
            .unwrap();
        let ping_count = addr.send(CountPings {}).await.unwrap();
        assert_eq!(
            ping_count, 2,
            "weak recipients must not fail to send a message"
        );
    });
}

#[test]
fn test_weak_addr_partial_equality() {
    let sys = System::new();

    sys.block_on(async move {
        let actor1 = MyActor3 {}.start();
        let actor2 = MyActor3 {}.start();

        let weak1 = actor1.downgrade();
        let weak1_again = actor1.downgrade();
        let weak2 = actor2.downgrade();

        // if this stops compiling this means that Add::downgrade
        // now takes self by value and we must clone before downgrading

        assert!(actor1.connected(), "actor 1 must be alive");
        assert!(actor2.connected(), "actor 2 must be alive");
        // the assertions we want to hold for partial equality of weak actor addresses
        // these assertions must hold whether one or both of the actors are connected or not
        let check_equality_assertions = || {
            assert_eq!(weak1, weak1);
            assert_eq!(weak1, weak1_again);
            assert_eq!(weak2, weak2);

            assert_ne!(weak1, weak2);
            assert_ne!(weak1_again, weak2);
        };
        check_equality_assertions();
        // make sure that the same results apply when upgrading weak addr to addr and
        // comparing strong addresses so the results are intuitive and consistent
        assert_eq!(weak1.upgrade().unwrap(), weak1.upgrade().unwrap());
        assert_eq!(weak1.upgrade().unwrap(), weak1_again.upgrade().unwrap());
        assert_eq!(weak2.upgrade().unwrap(), weak2.upgrade().unwrap());
        assert_ne!(weak1.upgrade().unwrap(), weak2.upgrade().unwrap());
        assert_ne!(weak1_again.upgrade().unwrap(), weak2.upgrade().unwrap());

        // now drop one of the actors and make sure the same equality comparisons still hold
        drop(actor1);
        assert!(actor2.connected());
        check_equality_assertions();

        // and drop the second actor as well
        drop(actor2);
        check_equality_assertions();
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
        sleep(Duration::from_millis(20)).into_actor(self).wait(ctx);
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
            let res = addr.send(Ping(0)).timeout(Duration::from_millis(1)).await;
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
