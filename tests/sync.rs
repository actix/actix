
extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use futures::{future, Future};
use tokio_core::reactor::Timeout;
use actix::prelude::*;


struct Fibonacci(pub u32);

struct SyncActor {
    counter: Arc<AtomicUsize>,
    messages: Arc<AtomicUsize>,
    addr: SyncAddress<System>,
}

impl Actor for SyncActor {
    type Context = SyncContext<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        self.counter.fetch_add(1, Ordering::Relaxed);
    }
}

impl ResponseType<Fibonacci> for SyncActor {
    type Item = u64;
    type Error = ();
}

impl Handler<Fibonacci> for SyncActor {
    fn handle(&mut self, msg: Fibonacci, _: &mut Self::Context) -> Response<Self, Fibonacci> {
        let old = self.messages.fetch_add(1, Ordering::Relaxed);
        if old == 4 {
            self.addr.send(msgs::SystemExit(0));
        }

        if msg.0 == 0 {
            Response::Error(())
        } else if msg.0 == 1 {
            Response::Reply(1)
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
	        Response::Reply(sum)
        }
    }
}

#[test]
fn test_sync() {
    let sys = System::new("test".to_owned());
    let counter = Arc::new(AtomicUsize::new(0));
    let messages = Arc::new(AtomicUsize::new(0));

    let counter_c = Arc::clone(&counter);
    let messages_c = Arc::clone(&messages);
    let s_addr = Arbiter::system();
    let addr = SyncArbiter::start(
        2, move|| SyncActor{counter: Arc::clone(&counter_c),
                            messages: Arc::clone(&messages_c),
                            addr: s_addr.clone()}
    );

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 1000), Arbiter::handle()).unwrap()
            .then(move |_| {
                for n in 5..10 {
                    addr.send(Fibonacci(n));
                }
                future::result(Ok(()))
            })
    );

    sys.run();
    assert_eq!(counter.load(Ordering::Relaxed), 2, "Not started");
    assert_eq!(messages.load(Ordering::Relaxed), 5, "Wrong number of messages");
}
