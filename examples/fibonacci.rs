//! Example of sync actor. It can be used for cpu bound tasks. Only one sync actor
//! runs within arbiter's thread. Sync actor processes one message at a time.
//! Sync arbiter can start multiple threads with separate instance of actor in each.

extern crate actix;
extern crate futures;

use actix::prelude::*;

struct Fibonacci(pub u32);

impl ResponseType for Fibonacci {
    type Item = u64;
    type Error = ();
}


struct SyncActor;

impl Actor for SyncActor {
    type Context = SyncContext<Self>;
}

impl Handler<Fibonacci> for SyncActor {
    fn handle(&mut self, msg: Fibonacci, _: &mut Self::Context) -> Response<Self, Fibonacci> {
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

fn main() {
    let sys = System::new("test");

    // start sync arbiter with 3 threads
    let addr = SyncArbiter::start(3, || SyncActor);

    // send 5 messages
    for n in 5..10 {
        addr.send(Fibonacci(n));
    }

    Arbiter::handle().spawn_fn(|| {
        Arbiter::system().send(msgs::SystemExit(0));
        futures::future::result(Ok(()))
    });

    sys.run();
}
