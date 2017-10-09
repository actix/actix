#![cfg_attr(feature="cargo-clippy", allow(let_unit_value))]
extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::time::Duration;
use futures::{future, Future};
use tokio_core::reactor::Timeout;
use actix::prelude::*;
use actix::msgs::SystemExit;

#[derive(PartialEq)]
enum Op {
    Cancel,
    Timeout,
    RunAfter,
}

struct MyActor{op: Op}

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<MyActor>) {
        match self.op {
            Op::Cancel => {
                let handle = ctx.add_timeout(TimeoutMessage, Duration::new(0, 100));
                ctx.cancel_future(handle);
            },
            Op::Timeout => {
                ctx.add_timeout(TimeoutMessage, Duration::new(0, 100));
            },
            Op::RunAfter => {
                ctx.run_after(Duration::new(0, 100), |_, _| {
                    Arbiter::system().send(SystemExit(0));
                });
            }
        }
    }
}

struct TimeoutMessage;

impl ResponseType<TimeoutMessage> for MyActor {
    type Item = ();
    type Error = ();
}

impl Handler<TimeoutMessage> for MyActor {
    fn handle(&mut self, _: TimeoutMessage, _: &mut Context<Self>)
              -> Response<Self, TimeoutMessage> {
        if self.op != Op::Timeout {
            assert!(false, "should not happen");
        }
        Arbiter::system().send(SystemExit(0));
        Self::empty()
    }
}

#[test]
fn test_add_timeout() {
    let sys = System::new("test");

    let _addr: Address<_> = MyActor{op: Op::Timeout}.start();

    sys.run();
}


#[test]
fn test_add_timeout_cancel() {
    let sys = System::new("test");

    let _addr: Address<_> = MyActor{op: Op::Cancel}.start();

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 1000), Arbiter::handle()).unwrap()
            .then(|_| {
                Arbiter::system().send(SystemExit(0));
                future::result(Ok(()))
            })
    );

    sys.run();
}

#[test]
fn test_run_after() {
    let sys = System::new("test");

    let _addr: Address<_> = MyActor{op: Op::RunAfter}.start();

    sys.run();
}
