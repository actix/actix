#![cfg_attr(feature="cargo-clippy", allow(let_unit_value))]
extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::time::Duration;
use futures::{future, Future};
use tokio_core::reactor::Timeout;
use actix::prelude::*;
use actix::msgs::SystemExit;

#[derive(Debug, PartialEq)]
enum Op {
    Cancel,
    Timeout,
    TimeoutStop,
    RunAfter,
    RunAfterStop,
}

struct MyActor{op: Op}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Context<MyActor>) {
        match self.op {
            Op::Cancel => {
                let handle = ctx.notify_later(TimeoutMessage, Duration::new(0, 100));
                ctx.cancel_future(handle);
            },
            Op::Timeout => {
                ctx.notify_later(TimeoutMessage, Duration::new(0, 1000));
            },
            Op::TimeoutStop => {
                ctx.notify_later(TimeoutMessage, Duration::new(0, 100000));
                ctx.stop();
            },
            Op::RunAfter => {
                ctx.run_later(Duration::new(0, 100), |_, _| {
                    Arbiter::system().send(SystemExit(0));
                });
            }
            Op::RunAfterStop => {
                ctx.run_later(Duration::new(1, 0), |_, _| {
                    panic!("error");
                });
                ctx.stop();
            }
        }
    }

    fn stopped(&mut self, _: &mut Context<MyActor>) {
        Arbiter::system().send(SystemExit(0));
    }
}

struct TimeoutMessage;

impl ResponseType for TimeoutMessage {
    type Item = ();
    type Error = ();
}

impl Handler<TimeoutMessage> for MyActor {
    type Result = ();

    fn handle(&mut self, _: TimeoutMessage, _: &mut Self::Context) {
        if self.op != Op::Timeout {
            assert!(false, "should not happen {:?}", self.op);
        }
        Arbiter::system().send(SystemExit(0));
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
fn test_add_timeout_stop() {
    let sys = System::new("test");

    let _addr: Address<_> = MyActor{op: Op::TimeoutStop}.start();

    sys.run();
}

#[test]
fn test_run_after() {
    let sys = System::new("test");

    let _addr: Address<_> = MyActor{op: Op::RunAfter}.start();

    sys.run();
}

#[test]
fn test_run_after_stop() {
    let sys = System::new("test");

    let _addr: Address<_> = MyActor{op: Op::RunAfterStop}.start();

    sys.run();
}
