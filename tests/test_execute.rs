extern crate actix;
extern crate futures;
use futures::{future, Future};

use actix::prelude::*;
use actix::msgs::{Execute, SystemExit};


#[test]
fn test_execute() {
    let sys = System::new("test");
    assert_eq!(Arbiter::name(), "test");

    let addr = Arbiter::new(None);

    Arbiter::handle().spawn(
        addr.call_fut(Execute::new(|| {
            Ok(Arbiter::name())
        })).then(|res: Result<Result<_, ()>, _>| {
            Arbiter::system().send(SystemExit(0));

            match res {
                Ok(Ok(name)) => assert_ne!(name, "test"),
                _ => assert!(false, "something is wrong"),
            }
            future::result(Ok(()))
        }));

    sys.run();
}

#[test]
fn test_system_execute() {
    let sys = System::new("test");
    assert_eq!(Arbiter::name(), "test");

    let addr = Arbiter::new(None);

    addr.send(Execute::new(
        || -> Result<(), ()> {
            Arbiter::handle().spawn_fn(|| {
                Arbiter::system_arbiter().send(Execute::new(|| -> Result<(), ()> {
                    Arbiter::system().send(SystemExit(0));

                    assert_eq!(Arbiter::name(), "test");
                    Ok(())
                }));
                future::ok(())
            });
            Ok(())
        })
    );

    sys.run();
}
