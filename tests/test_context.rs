#![cfg_attr(feature = "cargo-clippy", allow(let_unit_value))]
#[macro_use]
extern crate actix;
extern crate futures;
extern crate tokio;
extern crate tokio_timer;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix::prelude::*;
use futures::stream::once;
use futures::unsync::mpsc::unbounded;
use futures::{future, Future, Stream};
use tokio_timer::{Delay, Interval};

#[derive(Debug, PartialEq)]
enum Op {
    Cancel,
    Timeout,
    TimeoutStop,
    RunAfter,
    RunAfterStop,
}

struct MyActor {
    op: Op,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Context<MyActor>) {
        match self.op {
            Op::Cancel => {
                let handle0 = ctx.notify_later(TimeoutMessage, Duration::new(0, 100));
                let handle1 = ctx.notify_later(TimeoutMessage, Duration::new(0, 100));
                assert!(ctx.cancel_future(handle1));
                assert!(ctx.cancel_future(handle0));
            }
            Op::Timeout => {
                ctx.notify_later(TimeoutMessage, Duration::new(0, 1000));
            }
            Op::TimeoutStop => {
                ctx.notify_later(TimeoutMessage, Duration::new(0, 1_000_000));
                ctx.stop();
            }
            Op::RunAfter => {
                ctx.run_later(Duration::new(0, 100), |_, _| {
                    System::current().stop();
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
        System::current().stop();
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
        System::current().stop();
    }
}

#[test]
fn test_add_timeout() {
    System::run(|| {
        let _addr = MyActor { op: Op::Timeout }.start();
    });
}

#[test]
fn test_add_timeout_cancel() {
    System::run(|| {
        let _addr = MyActor { op: Op::Cancel }.start();

        tokio::spawn(
            Delay::new(Instant::now() + Duration::new(0, 1000)).then(|_| {
                System::current().stop();
                future::result(Ok(()))
            }),
        );
    });
}

#[test]
// delayed notification should be dropped after context stop
fn test_add_timeout_stop() {
    System::run(|| {
        let _addr = MyActor {
            op: Op::TimeoutStop,
        }.start();
    });
}

#[test]
fn test_run_after() {
    System::run(|| {
        let _addr = MyActor { op: Op::RunAfter }.start();
    });
}

#[test]
fn test_run_after_stop() {
    System::run(|| {
        let _addr = MyActor {
            op: Op::RunAfterStop,
        }.start();
    });
}

struct ContextWait {
    cnt: Arc<AtomicUsize>,
}

impl Actor for ContextWait {
    type Context = actix::Context<Self>;
}

#[derive(Message)]
struct Ping;

impl Handler<Ping> for ContextWait {
    type Result = ();

    fn handle(&mut self, _: Ping, ctx: &mut Self::Context) {
        let cnt = self.cnt.load(Ordering::Relaxed);
        self.cnt.store(cnt + 1, Ordering::Relaxed);

        let fut = Delay::new(Instant::now() + Duration::from_secs(10));
        fut.map_err(|_| ()).map(|_| ()).into_actor(self).wait(ctx);

        System::current().stop();
    }
}

#[test]
fn test_wait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let cnt = Arc::clone(&m);

    System::run(move || {
        let addr = ContextWait { cnt }.start();
        addr.do_send(Ping);
        addr.do_send(Ping);
        addr.do_send(Ping);
    });

    assert_eq!(m.load(Ordering::Relaxed), 1);
}

#[test]
fn test_message_stream_wait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    System::run(move || {
        let _addr = ContextWait::create(move |ctx| {
            let (tx, rx) = unbounded();
            let _ = tx.unbounded_send(Ping);
            let _ = tx.unbounded_send(Ping);
            let _ = tx.unbounded_send(Ping);
            let actor = ContextWait { cnt: m2 };
            ctx.add_message_stream(rx);
            actor
        });
    });

    assert_eq!(m.load(Ordering::Relaxed), 1);
}

#[test]
fn test_stream_wait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    System::run(move || {
        let _addr = ContextWait::create(move |ctx| {
            let (tx, rx) = unbounded();
            let _ = tx.unbounded_send(Ping);
            let _ = tx.unbounded_send(Ping);
            let _ = tx.unbounded_send(Ping);
            let actor = ContextWait { cnt: m2 };
            ctx.add_message_stream(rx);
            actor
        });
    });

    assert_eq!(m.load(Ordering::Relaxed), 1);
}

struct ContextNoWait {
    cnt: Arc<AtomicUsize>,
}

impl Actor for ContextNoWait {
    type Context = actix::Context<Self>;
}

impl Handler<Ping> for ContextNoWait {
    type Result = ();

    fn handle(&mut self, _: Ping, _: &mut Self::Context) {
        let cnt = self.cnt.load(Ordering::Relaxed);
        self.cnt.store(cnt + 1, Ordering::Relaxed);
    }
}

#[test]
fn test_nowait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let cnt = Arc::clone(&m);

    System::run(move || {
        let addr = ContextNoWait { cnt }.start();
        addr.do_send(Ping);
        addr.do_send(Ping);
        addr.do_send(Ping);

        tokio::spawn(
            Delay::new(Instant::now() + Duration::from_millis(200))
                .map_err(|_| ())
                .map(|_| System::current().stop()),
        );
    });

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[test]
fn test_message_stream_nowait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    System::run(move || {
        let _addr = ContextNoWait::create(move |ctx| {
            let (tx, rx) = unbounded();
            let _ = tx.unbounded_send(Ping);
            let _ = tx.unbounded_send(Ping);
            let _ = tx.unbounded_send(Ping);
            let actor = ContextNoWait { cnt: m2 };
            ctx.add_message_stream(rx);
            actor
        });
        tokio::spawn(
            Delay::new(Instant::now() + Duration::from_millis(200))
                .map_err(|_| ())
                .map(|_| System::current().stop()),
        );
    });

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[test]
fn test_stream_nowait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    System::run(move || {
        let _addr = ContextNoWait::create(move |ctx| {
            let (tx, rx) = unbounded();
            let _ = tx.unbounded_send(Ping);
            let _ = tx.unbounded_send(Ping);
            let _ = tx.unbounded_send(Ping);
            let actor = ContextNoWait { cnt: m2 };
            ctx.add_message_stream(rx);
            actor
        });

        tokio::spawn(
            Delay::new(Instant::now() + Duration::from_millis(200))
                .map_err(|_| ())
                .map(|_| System::current().stop()),
        );
    });

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[test]
fn test_notify() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    System::run(move || {
        let addr = ContextNoWait::create(move |ctx| {
            ctx.notify(Ping);
            ctx.notify(Ping);
            ContextNoWait {
                cnt: Arc::clone(&m),
            }
        });
        addr.do_send(Ping);

        tokio::spawn(
            Delay::new(Instant::now() + Duration::from_millis(200))
                .map_err(|_| ())
                .map(|_| {
                    System::current().stop();
                }),
        );
    });

    assert_eq!(m2.load(Ordering::Relaxed), 3);
}

struct ContextHandle {
    h: Arc<AtomicUsize>,
}
impl Actor for ContextHandle {
    type Context = Context<Self>;
}

impl StreamHandler2<Ping, ()> for ContextHandle {
    fn handle(&mut self, _: Result<Option<Ping>, ()>, ctx: &mut Self::Context) {
        self.h.store(ctx.handle().into_usize(), Ordering::Relaxed);
        System::current().stop();
    }
}

#[test]
fn test_current_context_handle() {
    let h = Arc::new(AtomicUsize::new(0));
    let h2 = Arc::clone(&h);
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    System::run(move || {
        let _addr = ContextHandle::create(move |ctx| {
            h2.store(
                ContextHandle::add_stream(once::<Ping, ()>(Ok(Ping)), ctx).into_usize(),
                Ordering::Relaxed,
            );

            ContextHandle { h: m2 }
        });
    });

    assert_eq!(m.load(Ordering::Relaxed), h.load(Ordering::Relaxed));
}

#[test]
fn test_start_from_context() {
    let h = Arc::new(AtomicUsize::new(0));
    let h2 = Arc::clone(&h);
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    System::run(move || {
        let _addr = ContextHandle::create(move |ctx| {
            h2.store(
                ctx.add_stream2(once::<Ping, ()>(Ok(Ping))).into_usize(),
                Ordering::Relaxed,
            );
            ContextHandle { h: m2 }
        });
    });

    assert_eq!(m.load(Ordering::Relaxed), h.load(Ordering::Relaxed));
}

struct CancelHandler {
    source: SpawnHandle,
}
impl Actor for CancelHandler {
    type Context = Context<Self>;

    fn stopped(&mut self, _: &mut Context<Self>) {
        System::current().stop();
    }
}

struct CancelPacket;
impl<K> StreamHandler2<CancelPacket, K> for CancelHandler {
    fn handle(&mut self, _: Result<Option<CancelPacket>, K>, ctx: &mut Context<Self>) {
        ctx.cancel_future(self.source);
    }
}

#[test]
fn test_cancel_handler() {
    actix::System::run(|| {
        CancelHandler::create(|ctx| CancelHandler {
            source: ctx.add_stream2(
                Interval::new(Instant::now(), Duration::from_millis(1))
                    .map(|_| CancelPacket),
            ),
        });
    });
}
