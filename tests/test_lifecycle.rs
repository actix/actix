#![cfg_attr(feature = "cargo-clippy", allow(let_unit_value))]

extern crate actix;
extern crate futures;
extern crate tokio_core;

use actix::msgs::SystemExit;
use actix::prelude::*;
use futures::unsync::oneshot::{channel, Sender};
use futures::{future, Future};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_core::reactor::Timeout;

struct MyActor {
    started: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    temp: Option<Sender<()>>,
    restore_after_stop: bool,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        self.started.store(true, Ordering::Relaxed);
    }
    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        self.stopping.store(true, Ordering::Relaxed);

        if self.restore_after_stop {
            let (tx, rx) = channel();
            self.temp = Some(tx);
            rx.actfuture()
                .then(|_, _: &mut MyActor, _: &mut _| actix::fut::result(Ok(())))
                .spawn(ctx);
            Running::Continue
        } else {
            Running::Stop
        }
    }
    fn stopped(&mut self, _: &mut Self::Context) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

struct MySyncActor {
    started: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    restore_after_stop: bool,
}

impl Actor for MySyncActor {
    type Context = actix::SyncContext<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        self.started.store(true, Ordering::Relaxed);
    }
    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        self.stopping.store(true, Ordering::Relaxed);

        if self.restore_after_stop {
            self.restore_after_stop = false;
            Running::Continue
        } else {
            Running::Stop
        }
    }
    fn stopped(&mut self, _: &mut Self::Context) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

#[test]
fn test_active_address() {
    let sys = System::new("test");

    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));

    let _addr: Addr<Unsync, _> = MyActor {
        started: Arc::clone(&started),
        stopping: Arc::clone(&stopping),
        stopped: Arc::clone(&stopped),
        temp: None,
        restore_after_stop: false,
    }.start();

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 100), Arbiter::handle())
            .unwrap()
            .then(|_| {
                Arbiter::system().do_send(SystemExit(0));
                future::result(Ok(()))
            }),
    );

    sys.run();
    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(!stopping.load(Ordering::Relaxed), "Stopping");
    assert!(!stopped.load(Ordering::Relaxed), "Stopped");
}

#[test]
fn test_active_sync_address() {
    let sys = System::new("test");

    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));

    let _addr: Addr<Syn, _> = MyActor {
        started: Arc::clone(&started),
        stopping: Arc::clone(&stopping),
        stopped: Arc::clone(&stopped),
        temp: None,
        restore_after_stop: false,
    }.start();

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 100), Arbiter::handle())
            .unwrap()
            .then(|_| {
                Arbiter::system().do_send(SystemExit(0));
                future::result(Ok(()))
            }),
    );

    sys.run();
    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(!stopping.load(Ordering::Relaxed), "Stopping");
    assert!(!stopped.load(Ordering::Relaxed), "Stopped");
}

#[test]
fn test_stop_after_drop_address() {
    let sys = System::new("test");

    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));

    let addr: Addr<Unsync, _> = MyActor {
        started: Arc::clone(&started),
        stopping: Arc::clone(&stopping),
        stopped: Arc::clone(&stopped),
        temp: None,
        restore_after_stop: false,
    }.start();

    let started2 = Arc::clone(&started);
    let stopping2 = Arc::clone(&stopping);
    let stopped2 = Arc::clone(&stopped);

    Arbiter::handle().spawn_fn(move || {
        assert!(started2.load(Ordering::Relaxed), "Not started");
        assert!(!stopping2.load(Ordering::Relaxed), "Stopping");
        assert!(!stopped2.load(Ordering::Relaxed), "Stopped");

        Timeout::new(Duration::new(0, 100), Arbiter::handle())
            .unwrap()
            .then(move |_| {
                drop(addr);
                Timeout::new(Duration::new(0, 10_000), Arbiter::handle())
                    .unwrap()
                    .then(|_| {
                        Arbiter::system().do_send(SystemExit(0));
                        future::result(Ok(()))
                    })
            })
    });

    sys.run();
    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(stopped.load(Ordering::Relaxed), "Not stopped");
}

#[test]
fn test_stop_after_drop_sync_address() {
    let sys = System::new("test");

    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));

    let addr: Addr<Syn, _> = MyActor {
        started: Arc::clone(&started),
        stopping: Arc::clone(&stopping),
        stopped: Arc::clone(&stopped),
        temp: None,
        restore_after_stop: false,
    }.start();

    let started2 = Arc::clone(&started);
    let stopping2 = Arc::clone(&stopping);
    let stopped2 = Arc::clone(&stopped);

    Arbiter::handle().spawn_fn(move || {
        assert!(started2.load(Ordering::Relaxed), "Not started");
        assert!(!stopping2.load(Ordering::Relaxed), "Stopping");
        assert!(!stopped2.load(Ordering::Relaxed), "Stopped");

        Timeout::new(Duration::new(0, 100), Arbiter::handle())
            .unwrap()
            .then(move |_| {
                drop(addr);
                Arbiter::system().do_send(SystemExit(0));
                future::result(Ok(()))
            })
    });

    sys.run();
    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(stopped.load(Ordering::Relaxed), "Not stopped");
}

#[test]
fn test_stop_after_drop_sync_actor() {
    let sys = System::new("test");

    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));

    let started1 = Arc::clone(&started);
    let stopping1 = Arc::clone(&stopping);
    let stopped1 = Arc::clone(&stopped);

    let addr: Addr<Syn, _> = SyncArbiter::start(1, move || MySyncActor {
        started: Arc::clone(&started1),
        stopping: Arc::clone(&stopping1),
        stopped: Arc::clone(&stopped1),
        restore_after_stop: false,
    });

    let started2 = Arc::clone(&started);
    let stopping2 = Arc::clone(&stopping);
    let stopped2 = Arc::clone(&stopped);

    Arbiter::handle().spawn_fn(move || {
        Timeout::new(Duration::from_secs(2), Arbiter::handle())
            .unwrap()
            .then(move |_| {
                assert!(started2.load(Ordering::Relaxed), "Not started");
                assert!(!stopping2.load(Ordering::Relaxed), "Stopping");
                assert!(!stopped2.load(Ordering::Relaxed), "Stopped");
                drop(addr);

                Timeout::new(Duration::from_secs(2), Arbiter::handle())
                    .unwrap()
                    .then(move |_| {
                        Arbiter::system().do_send(SystemExit(0));
                        future::result(Ok(()))
                    })
            })
    });

    sys.run();
    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(stopped.load(Ordering::Relaxed), "Not stopped");
}

#[test]
fn test_stop() {
    let sys = System::new("test");

    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));

    let _: () = MyActor {
        started: Arc::clone(&started),
        stopping: Arc::clone(&stopping),
        stopped: Arc::clone(&stopped),
        temp: None,
        restore_after_stop: false,
    }.start();

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 100), Arbiter::handle())
            .unwrap()
            .then(|_| {
                Arbiter::system().do_send(SystemExit(0));
                future::result(Ok(()))
            }),
    );

    sys.run();
    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(stopped.load(Ordering::Relaxed), "Not stopped");
}

#[test]
fn test_stop_restore_after_stopping() {
    let sys = System::new("test");

    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));

    let _: () = MyActor {
        started: Arc::clone(&started),
        stopping: Arc::clone(&stopping),
        stopped: Arc::clone(&stopped),
        temp: None,
        restore_after_stop: true,
    }.start();

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 100), Arbiter::handle())
            .unwrap()
            .then(|_| {
                Arbiter::system().do_send(SystemExit(0));
                future::result(Ok(()))
            }),
    );

    sys.run();
    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(!stopped.load(Ordering::Relaxed), "Stopped");
}
