extern crate actix;
extern crate futures;
extern crate tokio;
use futures::{future, Future};

use actix::msgs::Execute;
use actix::prelude::*;

#[test]
fn test_execute() {
    System::run(move || {
        let addr = Arbiter::new("exec-test");

        tokio::spawn(addr.send(Execute::new(|| Ok(Arbiter::name()))).then(
            |res: Result<Result<_, ()>, _>| {
                System::current().stop();

                match res {
                    Ok(Ok(name)) => assert_ne!(name, "test"),
                    _ => assert!(false, "something is wrong"),
                }
                future::result(Ok(()))
            },
        ));
    });
}

#[test]
fn test_system_execute() {
    System::run(move || {
        let addr = Arbiter::new("exec-test");

        addr.do_send(Execute::new(|| -> Result<(), ()> {
            tokio::spawn(futures::lazy(|| {
                System::current().arbiter().do_send(Execute::new(
                    || -> Result<(), ()> {
                        System::current().stop();
                        Ok(())
                    },
                ));
                future::ok(())
            }));
            Ok(())
        }));
    });
}
