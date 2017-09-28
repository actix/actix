extern crate actix;
extern crate futures;
use futures::{future, Future};

use actix::prelude::*;


#[test]
fn test_execute() {
    let sys = System::new("test".to_owned());
    assert_eq!(Arbiter::name(), "test");

    let addr = Arbiter::new(None);

    Arbiter::handle().spawn(
        addr.call_fut(actix::Execute::new(|| {
            Ok(Arbiter::name())
        })).then(|res: Result<Result<_, ()>, _>| {
            Arbiter::system().send(actix::SystemExit(0));

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
    let sys = System::new("test".to_owned());
    assert_eq!(Arbiter::name(), "test");

    let addr = Arbiter::new(None);

    addr.send(actix::Execute::new(
        || -> Result<(), ()> {
            Arbiter::handle().spawn_fn(|| {
                Arbiter::system().send(actix::Execute::new(|| -> Result<(), ()> {
                    Arbiter::system().send(actix::SystemExit(0));

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
