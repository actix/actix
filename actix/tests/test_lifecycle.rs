#![allow(clippy::let_unit_value)]

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use actix::prelude::*;
use actix_rt::time::sleep;
use tokio::sync::oneshot::{channel, Sender};

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
            ctx.spawn(
                async move {
                    let _ = rx.await;
                }
                .into_actor(self),
            );
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

    let _ = std::thread::spawn(move || {
        let sys = System::new();
        sys.block_on(async move {
            *addr2.lock().unwrap() = Some(
                MyActor {
                    started: started1,
                    stopping: stopping1,
                    stopped: stopped1,
                    temp: None,
                    restore_after_stop: false,
                }
                .start(),
            );
        });
        sys.run().unwrap();
    });
    std::thread::sleep(Duration::from_millis(100));

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

    System::new().block_on(async move {
        let addr = MyActor {
            started: started1,
            stopping: stopping1,
            stopped: stopped1,
            temp: None,
            restore_after_stop: false,
        }
        .start();

        actix_rt::spawn(async move {
            sleep(Duration::new(0, 100)).await;
            drop(addr);
            sleep(Duration::new(0, 10_000)).await;
            System::current().stop();
        });
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

    System::new().block_on(async move {
        let addr = MyActor {
            started: started1,
            stopping: stopping1,
            stopped: stopped1,
            temp: None,
            restore_after_stop: false,
        }
        .start();

        actix_rt::spawn(async move {
            sleep(Duration::new(0, 100)).await;
            drop(addr);
            System::current().stop();
        });
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

    System::new().block_on(async move {
        let addr = SyncArbiter::start(1, move || MySyncActor {
            started: Arc::clone(&started1),
            stopping: Arc::clone(&stopping1),
            stopped: Arc::clone(&stopped1),
            restore_after_stop: false,
        });

        actix_rt::spawn(async move {
            sleep(Duration::from_secs(2)).await;
            assert!(started2.load(Ordering::Relaxed), "Not started");
            assert!(!stopping2.load(Ordering::Relaxed), "Stopping");
            assert!(!stopped2.load(Ordering::Relaxed), "Stopped");
            drop(addr);

            sleep(Duration::from_secs(2)).await;
        })
        .await
        .unwrap();

        System::current().stop();
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

    System::new().block_on(async move {
        MyActor {
            started: started1,
            stopping: stopping1,
            stopped: stopped1,
            temp: None,
            restore_after_stop: false,
        }
        .start();

        actix_rt::spawn(async move {
            sleep(Duration::new(0, 100)).await;
            System::current().stop();
        });
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

    System::new().block_on(async move {
        MyActor {
            started: started1,
            stopping: stopping1,
            stopped: stopped1,
            temp: None,
            restore_after_stop: true,
        }
        .start();

        actix_rt::spawn(async move {
            sleep(Duration::new(0, 100)).await;
            System::current().stop();
        });
    });

    assert!(started.load(Ordering::Relaxed), "Not started");
    assert!(stopping.load(Ordering::Relaxed), "Not stopping");
    assert!(!stopped.load(Ordering::Relaxed), "Stopped");
}
