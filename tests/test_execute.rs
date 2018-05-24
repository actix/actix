extern crate actix;
extern crate futures;
use futures::{future, Future};

use actix::msgs::{Execute, SystemExit};
use actix::prelude::*;

#[test]
fn test_execute() {
    let sys = System::new("test");
    assert_eq!(Arbiter::name(), "test");

    let addr = Arbiter::new("exec-test");

    Arbiter::spawn(addr.send(Execute::new(|| Ok(Arbiter::name()))).then(
        |res: Result<Result<_, ()>, _>| {
            Arbiter::system().do_send(SystemExit(0));

            match res {
                Ok(Ok(name)) => assert_ne!(name, "test"),
                _ => assert!(false, "something is wrong"),
            }
            future::result(Ok(()))
        },
    ));

    sys.run();
}

#[test]
fn test_system_execute() {
    let sys = System::new("test");
    assert_eq!(Arbiter::name(), "test");

    let addr = Arbiter::new("exec-test");

    addr.do_send(Execute::new(|| -> Result<(), ()> {
        Arbiter::spawn_fn(|| {
            Arbiter::system_arbiter().do_send(Execute::new(|| -> Result<(), ()> {
                Arbiter::system().do_send(SystemExit(0));

                assert_eq!(Arbiter::name(), "test");
                Ok(())
            }));
            future::ok(())
        });
        Ok(())
    }));

    sys.run();
}
