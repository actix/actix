extern crate actix;
extern crate futures;
extern crate tokio;
use futures::{future, Future};

use actix::msgs::{Execute, SystemExit};
use actix::prelude::*;

#[test]
fn test_execute() {
    System::run(move || {
        let addr = Arbiter::new("exec-test");
        assert_eq!(Arbiter::name(), "actix");

        tokio::spawn(addr.send(Execute::new(|| Ok(Arbiter::name()))).then(
            |res: Result<Result<_, ()>, _>| {
                Arbiter::system().do_send(SystemExit(0));

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
                Arbiter::system_arbiter().do_send(Execute::new(|| -> Result<(), ()> {
                    Arbiter::system().do_send(SystemExit(0));

                    assert_eq!(Arbiter::name(), "actix");
                    Ok(())
                }));
                future::ok(())
            }));
            Ok(())
        }));
    });
}
