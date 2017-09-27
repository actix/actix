extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use futures::{future, Future};
use tokio_core::reactor::Timeout;
use actix::prelude::*;

struct Die;

struct MyActor(Arc<AtomicUsize>);

impl Actor for MyActor {
    fn started(&mut self, _: &mut Context<MyActor>) {
        let n = self.0.load(Ordering::Relaxed);
        self.0.store(n+1, Ordering::Relaxed);
    }
}

struct MyActorFactory(Arc<AtomicUsize>);

impl ActorFactory<MyActor> for MyActorFactory {
    fn create(&mut self, _: &mut Context<MyActor>) -> MyActor {
        MyActor(Arc::clone(&self.0))
    }
}

impl MessageResponse<Die> for MyActor {
    type Item = ();
    type Error = ();
}

impl MessageHandler<Die> for MyActor {

    fn handle(&mut self, _: Die, ctx: &mut Context<MyActor>) -> MessageFuture<Self, Die> {
        ctx.stop();
        ().to_result()
    }
}


#[test]
fn test_supervisor() {
    let sys = System::new("test".to_owned());

    let num = Arc::new(AtomicUsize::new(0));
    let factory = MyActorFactory(Arc::clone(&num));
    let (addr, _) = Supervisor::start(factory, false);

    addr.send(Die);

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 100), Arbiter::handle()).unwrap()
            .then(|_| {
                Arbiter::system().send(actix::SystemExit(0));
                future::result(Ok(()))
            })
    );

    sys.run();
    assert_eq!(num.load(Ordering::Relaxed), 2);
}

#[test]
fn test_supervisor_lazy() {
    let sys = System::new("test".to_owned());

    let num = Arc::new(AtomicUsize::new(0));
    let factory = MyActorFactory(Arc::clone(&num));
    let (addr, _) = Supervisor::start(factory, true);

    let num_2 = Arc::clone(&num);
    Arbiter::handle().spawn_fn(move || {
        assert_eq!(num_2.load(Ordering::Relaxed), 0);
        addr.send(Die);

        Timeout::new(Duration::new(0, 100), Arbiter::handle()).unwrap()
            .then(|_| {
                Arbiter::system().send(actix::SystemExit(0));
                future::result(Ok(()))
            })
    });

    sys.run();
    assert_eq!(num.load(Ordering::Relaxed), 2);
}

#[test]
fn test_supervisor_upgrade_address() {
    let sys = System::new("test".to_owned());

    let num = Arc::new(AtomicUsize::new(0));
    let factory = MyActorFactory(Arc::clone(&num));

    // lazy supervisor
    let (addr, _) = Supervisor::start(factory, true);

    Arbiter::handle().spawn_fn(move || {
        // upgrade address to SyncAddress
        Arbiter::handle().spawn(addr.upgrade().then(|res| {
            res.unwrap().send(Die);
            future::result(Ok(()))
        }));

        Timeout::new(Duration::new(0, 100), Arbiter::handle()).unwrap()
            .then(|_| {
                Arbiter::system().send(actix::SystemExit(0));
                future::result(Ok(()))
            })
    });

    sys.run();
    assert_eq!(num.load(Ordering::Relaxed), 2);
}
