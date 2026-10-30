use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Condvar, Mutex,
};

use actix::prelude::*;

struct Fibonacci(pub u32);

impl Message for Fibonacci {
    type Result = Result<u64, ()>;
}

struct SyncActor {
    cond: Arc<Condvar>,
    cond_l: Arc<Mutex<bool>>,
    counter: Arc<AtomicUsize>,
    messages: Arc<AtomicUsize>,
}

impl Actor for SyncActor {
    type Context = SyncContext<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        let old = self.counter.fetch_add(1, Ordering::Relaxed);
        if old == 1 {
            *self.cond_l.lock().unwrap() = true;
            self.cond.notify_one();
        }
    }
}

impl Handler<Fibonacci> for SyncActor {
    type Result = Result<u64, ()>;

    fn handle(&mut self, msg: Fibonacci, _: &mut Self::Context) -> Self::Result {
        let old = self.messages.fetch_add(1, Ordering::Relaxed);
        if old == 4 {
            System::current().stop();
        }

        if msg.0 == 0 {
            Err(())
        } else if msg.0 == 1 {
            Ok(1)
        } else {
            let mut i = 0;
            let mut sum = 0;
            let mut last = 0;
            let mut curr = 1;
            while i < msg.0 - 1 {
                sum = last + curr;
                last = curr;
                curr = sum;
                i += 1;
            }
            Ok(sum)
        }
    }
}

#[test]
#[allow(clippy::mutex_atomic)]
fn test_sync() {
    let l = Arc::new(Mutex::new(false));
    let cond = Arc::new(Condvar::new());
    let counter = Arc::new(AtomicUsize::new(0));
    let messages = Arc::new(AtomicUsize::new(0));

    let cond_c = Arc::clone(&cond);
    let cond_l_c = Arc::clone(&l);
    let counter_c = Arc::clone(&counter);
    let messages_c = Arc::clone(&messages);

    let sys = System::new();
    sys.block_on(async move {
        let addr = SyncArbiter::start(2, move || SyncActor {
            cond: Arc::clone(&cond_c),
            cond_l: Arc::clone(&cond_l_c),
            counter: Arc::clone(&counter_c),
            messages: Arc::clone(&messages_c),
        });

        let mut started = l.lock().unwrap();
        while !*started {
            started = cond.wait(started).unwrap();
        }

        for n in 5..10 {
            addr.do_send(Fibonacci(n));
        }
    });

    sys.run().unwrap();

    assert_eq!(counter.load(Ordering::Relaxed), 2, "Not started");
    assert_eq!(
        messages.load(Ordering::Relaxed),
        5,
        "Wrong number of messages"
    );
}

struct ValueActor {
    value: String,
}

impl Actor for ValueActor {
    type Context = SyncContext<Self>;
}

struct GetValue;

impl Message for GetValue {
    type Result = String;
}

impl Handler<GetValue> for ValueActor {
    type Result = String;

    fn handle(&mut self, _: GetValue, _: &mut Self::Context) -> Self::Result {
        self.value.clone()
    }
}

#[test]
fn test_start_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_c = Arc::clone(&calls);
    let value = String::from("consumed once");

    System::new().block_on(async move {
        let addr = SyncArbiter::start_once(move || {
            calls_c.fetch_add(1, Ordering::Relaxed);
            ValueActor { value }
        });

        assert_eq!(addr.send(GetValue).await.unwrap(), "consumed once");
        assert_eq!(addr.send(GetValue).await.unwrap(), "consumed once");

        System::current().stop();
    });

    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "factory ran more than once"
    );
}

struct PanicActor;

impl Actor for PanicActor {
    type Context = SyncContext<Self>;
}

struct Panic;

impl Message for Panic {
    type Result = ();
}

impl Handler<Panic> for PanicActor {
    type Result = ();

    fn handle(&mut self, _: Panic, _: &mut Self::Context) -> Self::Result {
        panic!("sync actor panic");
    }
}

#[test]
fn test_panic_stops_system() {
    let sys = System::new();

    let addr = sys.block_on(async { SyncArbiter::start(1, || PanicActor) });
    addr.do_send(Panic);

    assert_eq!(sys.run_with_code().unwrap(), 1);
}

struct DrainActor {
    handled: Arc<AtomicUsize>,
    total: usize,
}

impl Actor for DrainActor {
    type Context = SyncContext<Self>;
}

struct DrainMsg;

impl Message for DrainMsg {
    type Result = ();
}

impl Handler<DrainMsg> for DrainActor {
    type Result = ();

    fn handle(&mut self, _: DrainMsg, _: &mut Self::Context) -> Self::Result {
        std::thread::sleep(std::time::Duration::from_millis(1));

        if self.handled.fetch_add(1, Ordering::SeqCst) + 1 == self.total {
            System::current().stop();
        }
    }
}

#[test]
fn drains_queued_messages_when_last_sender_drops() {
    const TOTAL: usize = 32;

    let handled = Arc::new(AtomicUsize::new(0));
    let handled_c = Arc::clone(&handled);
    let sys = System::new();

    sys.block_on(async move {
        let addr = SyncArbiter::start(1, move || DrainActor {
            handled: Arc::clone(&handled_c),
            total: TOTAL,
        });

        for _ in 0..TOTAL {
            addr.do_send(DrainMsg);
        }

        // dropping the last sender closes the address while messages are
        // still queued; the arbiter must forward and process all of them.
        drop(addr);

        // fallback stop so a regression fails the count assertion instead of
        // hanging the test forever
        actix_rt::spawn(async {
            actix_rt::time::sleep(std::time::Duration::from_secs(5)).await;
            System::current().stop();
        });
    });

    sys.run().unwrap();

    assert_eq!(handled.load(Ordering::SeqCst), TOTAL);
}

#[test]
fn try_start_once_propagates_factory_error() {
    let result =
        SyncArbiter::<ValueActor>::try_start_once(|| Err::<ValueActor, _>("factory failed"));

    assert_eq!(result.err(), Some("factory failed"));
}

#[test]
fn try_start_once_starts_actor() {
    System::new().block_on(async {
        let addr = SyncArbiter::<ValueActor>::try_start_once(|| {
            Ok::<_, ()>(ValueActor {
                value: String::from("fallible"),
            })
        })
        .unwrap();

        assert_eq!(addr.send(GetValue).await.unwrap(), "fallible");

        System::current().stop();
    });
}

struct CountingActor {
    handled: Arc<AtomicUsize>,
}

impl Actor for CountingActor {
    type Context = SyncContext<Self>;
}

struct CountingMsg;

impl Message for CountingMsg {
    type Result = ();
}

impl Handler<CountingMsg> for CountingActor {
    type Result = ();

    fn handle(&mut self, _: CountingMsg, _: &mut SyncContext<Self>) -> Self::Result {
        std::thread::sleep(std::time::Duration::from_millis(1));
        self.handled.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn handle_waits_for_workers_to_drain() {
    const TOTAL: usize = 16;

    let sys = System::new();
    sys.block_on(async {
        let handled = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&handled);

        let (addr, handle) = SyncArbiter::start_with_handle(2, move || CountingActor {
            handled: Arc::clone(&counter),
        });

        for _ in 0..TOTAL {
            addr.do_send(CountingMsg);
        }
        drop(addr);

        actix_rt::time::timeout(std::time::Duration::from_secs(5), handle.wait())
            .await
            .expect("workers did not finish");

        assert_eq!(handled.load(Ordering::SeqCst), TOTAL);

        System::current().stop();
    });
}

#[test]
fn start_once_with_handle_waits_for_drain() {
    let sys = System::new();
    sys.block_on(async {
        let handled = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&handled);

        let (addr, handle) = SyncArbiter::start_once_with_handle(move || CountingActor {
            handled: Arc::clone(&counter),
        });

        for _ in 0..4 {
            addr.do_send(CountingMsg);
        }
        drop(addr);

        actix_rt::time::timeout(std::time::Duration::from_secs(5), handle.wait())
            .await
            .expect("worker did not finish");

        assert_eq!(handled.load(Ordering::SeqCst), 4);

        System::current().stop();
    });
}
