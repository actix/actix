#![cfg_attr(feature="cargo-clippy", allow(let_unit_value))]
extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::time::Duration;
use futures::{future, Future};
use tokio_core::reactor::Timeout;
use actix::prelude::*;
use actix::msgs::SystemExit;

struct MyActor{cancel: bool}

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<MyActor>) {
        let handle = ctx.add_timeout(TimeoutMessage, Duration::new(0, 100));
        if self.cancel {
            ctx.cancel_future(handle);
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
        if self.cancel {
            assert!(false, "should not happen");
        }
        Arbiter::system().send(SystemExit(0));
        Self::empty()
    }
}

#[test]
fn test_add_timeout() {
    let sys = System::new("test");

    let _addr: Address<_> = MyActor{cancel: false}.start();

    sys.run();
}


#[test]
fn test_add_timeout_cancel() {
    let sys = System::new("test");

    let _addr: Address<_> = MyActor{cancel: false}.start();

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 1000), Arbiter::handle()).unwrap()
            .then(|_| {
                Arbiter::system().send(SystemExit(0));
                future::result(Ok(()))
            })
    );

    sys.run();
}
