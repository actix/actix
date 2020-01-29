use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

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

    System::run(move || {
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
    })
    .unwrap();

    assert_eq!(counter.load(Ordering::Relaxed), 2, "Not started");
    assert_eq!(
        messages.load(Ordering::Relaxed),
        5,
        "Wrong number of messages"
    );
}
