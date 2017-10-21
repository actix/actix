extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::sync::{Arc, Condvar, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use futures::future;
use actix::prelude::*;


struct Fibonacci(pub u32);

impl ResponseType for Fibonacci {
    type Item = u64;
    type Error = ();
}

struct SyncActor {
    cond: Arc<Condvar>,
    cond_l: Arc<Mutex<bool>>,
    counter: Arc<AtomicUsize>,
    messages: Arc<AtomicUsize>,
    addr: SyncAddress<System>,
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
    fn handle(&mut self, msg: Fibonacci, _: &mut Self::Context) -> Response<Self, Fibonacci> {
        let old = self.messages.fetch_add(1, Ordering::Relaxed);
        if old == 4 {
            self.addr.send(msgs::SystemExit(0));
        }

        if msg.0 == 0 {
            Self::reply_error(())
        } else if msg.0 == 1 {
            Self::reply(1)
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
	        Self::reply(sum)
        }
    }
}

#[test]
#[cfg_attr(feature="cargo-clippy", allow(mutex_atomic))]
fn test_sync() {
    let sys = System::new("test");
    let l = Arc::new(Mutex::new(false));
    let cond = Arc::new(Condvar::new());
    let counter = Arc::new(AtomicUsize::new(0));
    let messages = Arc::new(AtomicUsize::new(0));

    let cond_c = Arc::clone(&cond);
    let cond_l_c = Arc::clone(&l);
    let counter_c = Arc::clone(&counter);
    let messages_c = Arc::clone(&messages);
    let s_addr = Arbiter::system();
    let addr = SyncArbiter::start(
        2, move|| SyncActor{cond: Arc::clone(&cond_c),
                            cond_l: Arc::clone(&cond_l_c),
                            counter: Arc::clone(&counter_c),
                            messages: Arc::clone(&messages_c),
                            addr: s_addr.clone()}
    );

    let mut started = l.lock().unwrap();
    while !*started {
        started = cond.wait(started).unwrap();
    }

    Arbiter::handle().spawn_fn(move || {
        for n in 5..10 {
            addr.send(Fibonacci(n));
        }
        future::result(Ok(()))
    });

    sys.run();
    assert_eq!(counter.load(Ordering::Relaxed), 2, "Not started");
    assert_eq!(messages.load(Ordering::Relaxed), 5, "Wrong number of messages");
}
