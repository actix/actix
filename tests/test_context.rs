#![cfg_attr(feature="cargo-clippy", allow(let_unit_value))]
#[macro_use] extern crate actix;
extern crate futures;
extern crate tokio_core;

use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use futures::{future, Future};
use futures::stream::once;
use futures::unsync::mpsc::unbounded;
use tokio_core::reactor::Timeout;
use actix::prelude::*;
use actix::msgs::SystemExit;

#[derive(Debug, PartialEq)]
enum Op {
    Cancel,
    Timeout,
    TimeoutStop,
    RunAfter,
    RunAfterStop,
}

struct MyActor{op: Op}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Context<MyActor>) {
        match self.op {
            Op::Cancel => {
                let handle = ctx.notify_later(TimeoutMessage, Duration::new(0, 100));
                ctx.cancel_future(handle);
            },
            Op::Timeout => {
                ctx.notify_later(TimeoutMessage, Duration::new(0, 1000));
            },
            Op::TimeoutStop => {
                ctx.notify_later(TimeoutMessage, Duration::new(0, 1_000_000));
                ctx.stop();
            },
            Op::RunAfter => {
                ctx.run_later(Duration::new(0, 100), |_, _| {
                    Arbiter::system().do_send(SystemExit(0));
                });
            }
            Op::RunAfterStop => {
                ctx.run_later(Duration::new(1, 0), |_, _| {
                    panic!("error");
                });
                ctx.stop();
            }
        }
    }

    fn stopped(&mut self, _: &mut Context<MyActor>) {
        Arbiter::system().do_send(SystemExit(0));
    }
}

#[derive(Message)]
struct TimeoutMessage;

impl Handler<TimeoutMessage> for MyActor {
    type Result = ();

    fn handle(&mut self, _: TimeoutMessage, _: &mut Self::Context) {
        if self.op != Op::Timeout {
            assert!(false, "should not happen {:?}", self.op);
        }
        Arbiter::system().do_send(SystemExit(0));
    }
}

#[test]
fn test_add_timeout() {
    let sys = System::new("test");

    let _addr: Addr<Unsync, _> = MyActor{op: Op::Timeout}.start();

    sys.run();
}


#[test]
fn test_add_timeout_cancel() {
    let sys = System::new("test");

    let _addr: Addr<Unsync, _> = MyActor{op: Op::Cancel}.start();

    Arbiter::handle().spawn(
        Timeout::new(Duration::new(0, 1000), Arbiter::handle()).unwrap()
            .then(|_| {
                Arbiter::system().do_send(SystemExit(0));
                future::result(Ok(()))
            })
    );

    sys.run();
}

#[test]
// delayed notification should be dropped after context stop
fn test_add_timeout_stop() {
    let sys = System::new("test");

    let _addr: Addr<Unsync, _> = MyActor{op: Op::TimeoutStop}.start();

    sys.run();
}

#[test]
fn test_run_after() {
    let sys = System::new("test");

    let _addr: Addr<Unsync, _> = MyActor{op: Op::RunAfter}.start();

    sys.run();
}

#[test]
fn test_run_after_stop() {
    let sys = System::new("test");

    let _addr: Addr<Unsync, _> = MyActor{op: Op::RunAfterStop}.start();

    sys.run();
}


struct ContextWait {cnt: Arc<AtomicUsize>}

impl Actor for ContextWait {
    type Context = actix::Context<Self>;
}

#[derive(Message)]
struct Ping;

impl Handler<Ping> for ContextWait {
    type Result = ();

    fn handle(&mut self, _: Ping, ctx: &mut Self::Context) {
        let cnt = self.cnt.load(Ordering::Relaxed);
        self.cnt.store(cnt+1, Ordering::Relaxed);

        let fut = Timeout::new(Duration::from_secs(10), Arbiter::handle()).unwrap();
        fut.map_err(|_| ()).map(|_| ())
            .into_actor(self)
            .wait(ctx);

        Arbiter::system().do_send(SystemExit(0));
    }
}

#[test]
fn test_wait_context() {
    let sys = System::new("test");

    let m = Arc::new(AtomicUsize::new(0));
    let addr: Addr<Unsync, _> = ContextWait{cnt: Arc::clone(&m)}.start();
    addr.do_send(Ping);
    addr.do_send(Ping);
    addr.do_send(Ping);

    sys.run();

    assert_eq!(m.load(Ordering::Relaxed), 1);
}

#[test]
fn test_message_stream_wait_context() {
    let sys = System::new("test");

    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);
    let _addr: Addr<Unsync, _> = ContextWait::create(move |ctx| {
        let (tx, rx) = unbounded();
        let _ = tx.unbounded_send(Ping);
        let _ = tx.unbounded_send(Ping);
        let _ = tx.unbounded_send(Ping);
        let actor = ContextWait{cnt: m2};
        ctx.add_message_stream(rx);
        actor
    });
    sys.run();

    assert_eq!(m.load(Ordering::Relaxed), 1);
}

#[test]
fn test_stream_wait_context() {
    let sys = System::new("test");

    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);
    let _addr: Addr<Unsync, _> = ContextWait::create(move |ctx| {
        let (tx, rx) = unbounded();
        let _ = tx.unbounded_send(Ping);
        let _ = tx.unbounded_send(Ping);
        let _ = tx.unbounded_send(Ping);
        let actor = ContextWait{cnt: m2};
        ctx.add_message_stream(rx);
        actor
    });
    sys.run();

    assert_eq!(m.load(Ordering::Relaxed), 1);
}

struct ContextNoWait {cnt: Arc<AtomicUsize>}

impl Actor for ContextNoWait {
    type Context = actix::Context<Self>;
}

impl Handler<Ping> for ContextNoWait {
    type Result = ();

    fn handle(&mut self, _: Ping, _: &mut Self::Context) {
        let cnt = self.cnt.load(Ordering::Relaxed);
        self.cnt.store(cnt+1, Ordering::Relaxed);

        Arbiter::system().do_send(SystemExit(0));
    }
}

#[test]
fn test_nowait_context() {
    let sys = System::new("test");

    let m = Arc::new(AtomicUsize::new(0));
    let addr: Addr<Unsync, _> = ContextNoWait{cnt: Arc::clone(&m)}.start();
    addr.do_send(Ping);
    addr.do_send(Ping);
    addr.do_send(Ping);
    sys.run();

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[test]
fn test_message_stream_nowait_context() {
    let sys = System::new("test");

    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);
    let _addr: Addr<Unsync, _> = ContextNoWait::create(move |ctx| {
        let (tx, rx) = unbounded();
        let _ = tx.unbounded_send(Ping);
        let _ = tx.unbounded_send(Ping);
        let _ = tx.unbounded_send(Ping);
        let actor = ContextNoWait{cnt: m2};
        ctx.add_message_stream(rx);
        actor
    });
    sys.run();

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[test]
fn test_stream_nowait_context() {
    let sys = System::new("test");

    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);
    let _addr: Addr<Unsync, _> = ContextNoWait::create(move |ctx| {
        let (tx, rx) = unbounded();
        let _ = tx.unbounded_send(Ping);
        let _ = tx.unbounded_send(Ping);
        let _ = tx.unbounded_send(Ping);
        let actor = ContextNoWait{cnt: m2};
        ctx.add_message_stream(rx);
        actor
    });
    sys.run();

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[test]
fn test_notify() {
    let sys = System::new("test");

    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);
    let addr: Addr<Unsync, _> = ContextNoWait::create(move |ctx| {
        ctx.notify(Ping);
        ctx.notify(Ping);
        ContextNoWait{cnt: Arc::clone(&m)}
    });
    addr.do_send(Ping);

    sys.run();
    assert_eq!(m2.load(Ordering::Relaxed), 3);
}


struct ContextHandle {h: Arc<AtomicUsize>}
impl Actor for ContextHandle {
    type Context = Context<Self>;
}

impl StreamHandler<Ping, ()> for ContextHandle {

    fn handle(&mut self, _: Ping, ctx: &mut Self::Context) {
        self.h.store(ctx.handle().into_usize(), Ordering::Relaxed);
        Arbiter::system().do_send(SystemExit(0));
    }
}

#[test]
fn test_current_context_handle() {
    let sys = System::new("test");

    let h = Arc::new(AtomicUsize::new(0));
    let h2 = Arc::clone(&h);
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    let _addr: Addr<Unsync, _> = ContextHandle::create(move |ctx| {
        h2.store(
            ContextHandle::add_stream(
                once::<Ping, ()>(Ok(Ping)), ctx).into_usize(), Ordering::Relaxed);

        ContextHandle{h: m2}
    });
    sys.run();

    assert_eq!(m.load(Ordering::Relaxed), h.load(Ordering::Relaxed));
}

#[test]
fn test_start_from_context() {
    let sys = System::new("test");

    let h = Arc::new(AtomicUsize::new(0));
    let h2 = Arc::clone(&h);
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    let _addr: Addr<Unsync, _> = ContextHandle::create(move |ctx| {
        h2.store(ctx.add_stream(
            once::<Ping, ()>(Ok(Ping))).into_usize(), Ordering::Relaxed);
        ContextHandle{h: m2}
    });
    sys.run();

    assert_eq!(m.load(Ordering::Relaxed), h.load(Ordering::Relaxed));
}
