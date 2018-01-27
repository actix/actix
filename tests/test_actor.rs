extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::time::Duration;
use actix::prelude::*;
use futures::{Future, future};
use tokio_core::reactor::Timeout;

#[derive(Debug)]
struct Num(usize);

impl ResponseType for Num {
    type Item = ();
    type Error = ();
}

struct MyActor(Arc<AtomicUsize>, Arc<AtomicBool>);

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn stopping(&mut self, _: &mut Self::Context) -> bool {
        println!("SEND");
        Arbiter::system().send(actix::msgs::SystemExit(0));
        true
    }
}

impl actix::Handler<Result<Num, ()>> for MyActor {
    type Result = ();

    fn handle(&mut self, msg: Result<Num, ()>, _: &mut actix::Context<MyActor>) {
        if let Ok(msg) = msg {
            self.0.fetch_add(msg.0, Ordering::Relaxed);
        } else {
            self.1.store(true, Ordering::Relaxed);
        }
    }
}

#[test]
fn test_stream() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));
    let err = Arc::new(AtomicBool::new(false));
    let items = vec![Num(1), Num(1), Num(1), Num(1), Num(1), Num(1), Num(1)];

    let act_count = Arc::clone(&count);
    MyActor::create::<(), _>(move |ctx| {
        ctx.add_stream(futures::stream::iter_ok::<_, ()>(items));
        MyActor(act_count, err)
    });

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 7);
}

#[test]
fn test_stream_with_error() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));
    let error = Arc::new(AtomicBool::new(false));
    let items = vec![Ok(Num(1)), Ok(Num(1)), Err(()), Ok(Num(1)),
                     Ok(Num(1)), Ok(Num(1)), Ok(Num(1)), Ok(Num(1))];

    let act_count = Arc::clone(&count);
    let act_error = Arc::clone(&error);
    MyActor::create::<(), _>(move |ctx| {
        ctx.add_stream(futures::stream::iter_result(items));
        MyActor(act_count, act_error)
    });

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 7);
    assert!(error.load(Ordering::Relaxed));
}


struct MySyncActor {
    started: Arc<AtomicUsize>,
    stopping: Arc<AtomicUsize>,
    stopped: Arc<AtomicUsize>,
    msgs: Arc<AtomicUsize>,
    stop: bool,
}

impl Actor for MySyncActor {
    type Context = actix::SyncContext<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        self.started.fetch_add(1, Ordering::Relaxed);
    }
    fn stopping(&mut self, _: &mut Self::Context) -> bool {
        self.stopping.fetch_add(1, Ordering::Relaxed);
        false
    }
    fn stopped(&mut self, _: &mut Self::Context) {
        self.stopped.fetch_add(1, Ordering::Relaxed);
    }
}

impl actix::Handler<Num> for MySyncActor {
    type Result = ();

    fn handle(&mut self, msg: Num, ctx: &mut Self::Context) {
        self.msgs.fetch_add(msg.0, Ordering::Relaxed);
        if self.stop {
            ctx.stop();
        }
    }
}

#[test]
fn test_restart_sync_actor() {
    let sys = System::new("test");

    let started = Arc::new(AtomicUsize::new(0));
    let stopping = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(AtomicUsize::new(0));
    let msgs = Arc::new(AtomicUsize::new(0));

    let started1 = Arc::clone(&started);
    let stopping1 = Arc::clone(&stopping);
    let stopped1 = Arc::clone(&stopped);
    let msgs1 = Arc::clone(&msgs);

    let addr: SyncAddress<_> = SyncArbiter::start(1, move || MySyncActor {
        started: Arc::clone(&started1),
        stopping: Arc::clone(&stopping1),
        stopped: Arc::clone(&stopped1),
        msgs: Arc::clone(&msgs1),
        stop: started1.load(Ordering::Relaxed) == 0});
    addr.send(Num(2));

    Arbiter::handle().spawn_fn(move || {
        addr.call_fut(Num(4))
            .then(move |_| {
                Timeout::new(Duration::new(0, 100_000), Arbiter::handle()).unwrap()
                    .then(move |_| {
                        Arbiter::system().send(actix::msgs::SystemExit(0));
                        future::result(Ok(()))
                    })
            })
    });

    sys.run();
    assert_eq!(started.load(Ordering::Relaxed), 2);
    assert_eq!(stopping.load(Ordering::Relaxed), 2);
    assert_eq!(stopped.load(Ordering::Relaxed), 2);
    assert_eq!(msgs.load(Ordering::Relaxed), 6);
}
