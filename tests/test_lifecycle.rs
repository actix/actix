#![cfg_attr(feature = "cargo-clippy", allow(let_unit_value))]

extern crate actix;
extern crate futures;
extern crate tokio;
extern crate tokio_timer;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use actix::prelude::*;
use futures::sync::oneshot::{channel, Sender};
use futures::{future, Future};
use tokio_timer::Delay;

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
    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let started1 = Arc::clone(&started);
    let stopping1 = Arc::clone(&stopping);
    let stopped1 = Arc::clone(&stopped);

    let addr = Arc::new(Mutex::new(None));
    let addr2 = Arc::clone(&addr);

    System::run(move || {
        *addr2.lock().unwrap() = Some(
            MyActor {
                started: started1,
                stopping: stopping1,
                stopped: stopped1,
                temp: None,
                restore_after_stop: false,
            }.start(),
        );

        tokio::spawn(
            Delay::new(Instant::now() + Duration::new(0, 100)).then(|_| {
                System::current().stop();
                future::result(Ok(()))
            }),
        );
    });

    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(!stopping.load(Ordering::Relaxed), "Stopping");
    assert!(!stopped.load(Ordering::Relaxed), "Stopped");
}

#[test]
fn test_stop_after_drop_address() {
    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let started1 = Arc::clone(&started);
    let stopping1 = Arc::clone(&stopping);
    let stopped1 = Arc::clone(&stopped);

    System::run(move || {
        let addr = MyActor {
            started: started1,
            stopping: stopping1,
            stopped: stopped1,
            temp: None,
            restore_after_stop: false,
        }.start();

        tokio::spawn(futures::lazy(move || {
            Delay::new(Instant::now() + Duration::new(0, 100)).then(move |_| {
                drop(addr);
                Delay::new(Instant::now() + Duration::new(0, 10_000)).then(|_| {
                    System::current().stop();
                    future::result(Ok(()))
                })
            })
        }));
    });

    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(stopped.load(Ordering::Relaxed), "Not stopped");
}

#[test]
fn test_stop_after_drop_sync_address() {
    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let started1 = Arc::clone(&started);
    let stopping1 = Arc::clone(&stopping);
    let stopped1 = Arc::clone(&stopped);

    System::run(move || {
        let addr = MyActor {
            started: started1,
            stopping: stopping1,
            stopped: stopped1,
            temp: None,
            restore_after_stop: false,
        }.start();

        tokio::spawn(futures::lazy(move || {
            Delay::new(Instant::now() + Duration::new(0, 100)).then(move |_| {
                drop(addr);
                System::current().stop();
                future::result(Ok(()))
            })
        }));
    });

    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(stopped.load(Ordering::Relaxed), "Not stopped");
}

#[test]
fn test_stop_after_drop_sync_actor() {
    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));

    let started1 = Arc::clone(&started);
    let stopping1 = Arc::clone(&stopping);
    let stopped1 = Arc::clone(&stopped);

    let started2 = Arc::clone(&started);
    let stopping2 = Arc::clone(&stopping);
    let stopped2 = Arc::clone(&stopped);

    System::run(move || {
        let addr = SyncArbiter::start(1, move || MySyncActor {
            started: Arc::clone(&started1),
            stopping: Arc::clone(&stopping1),
            stopped: Arc::clone(&stopped1),
            restore_after_stop: false,
        });

        tokio::spawn(futures::lazy(move || {
            Delay::new(Instant::now() + Duration::from_secs(2)).then(move |_| {
                assert!(started2.load(Ordering::Relaxed), "Not started");
                assert!(!stopping2.load(Ordering::Relaxed), "Stopping");
                assert!(!stopped2.load(Ordering::Relaxed), "Stopped");
                drop(addr);

                Delay::new(Instant::now() + Duration::from_secs(2)).then(move |_| {
                    System::current().stop();
                    future::result(Ok(()))
                })
            })
        }));
    });

    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(stopped.load(Ordering::Relaxed), "Not stopped");
}

#[test]
fn test_stop() {
    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let started1 = Arc::clone(&started);
    let stopping1 = Arc::clone(&stopping);
    let stopped1 = Arc::clone(&stopped);

    System::run(move || {
        MyActor {
            started: started1,
            stopping: stopping1,
            stopped: stopped1,
            temp: None,
            restore_after_stop: false,
        }.start();

        tokio::spawn(
            Delay::new(Instant::now() + Duration::new(0, 100)).then(|_| {
                System::current().stop();
                future::result(Ok(()))
            }),
        );
    });

    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(stopped.load(Ordering::Relaxed), "Not stopped");
}

#[test]
fn test_stop_restore_after_stopping() {
    let started = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let started1 = Arc::clone(&started);
    let stopping1 = Arc::clone(&stopping);
    let stopped1 = Arc::clone(&stopped);

    System::run(move || {
        MyActor {
            started: started1,
            stopping: stopping1,
            stopped: stopped1,
            temp: None,
            restore_after_stop: true,
        }.start();

        tokio::spawn(
            Delay::new(Instant::now() + Duration::new(0, 100)).then(|_| {
                System::current().stop();
                future::result(Ok(()))
            }),
        );
    });

    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(!stopped.load(Ordering::Relaxed), "Stopped");
}
