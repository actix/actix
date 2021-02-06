#![allow(clippy::let_unit_value)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context as StdContext, Poll};
use std::{pin::Pin, time::Duration};

use actix::prelude::*;
use actix_rt::time::{interval_at, sleep, Instant};
use futures_core::stream::Stream;
use futures_util::stream::once;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

#[derive(Debug, PartialEq)]
enum Op {
    Cancel,
    Timeout,
    TimeoutStop,
    RunAfter,
    RunAfterStop,
}

struct StreamRx {
    rx: UnboundedReceiver<Ping>,
}

impl Stream for StreamRx {
    type Item = Ping;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut StdContext<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        Pin::new(&mut this.rx).poll_recv(cx)
    }
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

struct TimeoutMessage;

impl Message for TimeoutMessage {
    type Result = ();
}

impl Handler<TimeoutMessage> for MyActor {
    type Result = ();

    fn handle(&mut self, _: TimeoutMessage, _: &mut Self::Context) {
        if self.op != Op::Timeout {
            panic!("should not happen {:?}", self.op);
        }
        System::current().stop();
    }
}

#[test]
fn test_add_timeout() {
    System::new().block_on(async {
        let _addr = MyActor { op: Op::Timeout }.start();
    });
}

#[test]
fn test_add_timeout_cancel() {
    System::new().block_on(async {
        let _addr = MyActor { op: Op::Cancel }.start();

        actix_rt::spawn(async move {
            sleep(Duration::new(0, 1000)).await;
            System::current().stop();
        });
    });
}

#[test]
// delayed notification should be dropped after context stop
fn test_add_timeout_stop() {
    System::new().block_on(async {
        let _addr = MyActor {
            op: Op::TimeoutStop,
        }
        .start();
    });
}

#[test]
fn test_run_after() {
    System::new().block_on(async {
        let _addr = MyActor { op: Op::RunAfter }.start();
    });
}

#[test]
fn test_run_after_stop() {
    System::new().block_on(async {
        let _addr = MyActor {
            op: Op::RunAfterStop,
        }
        .start();
    });
}

struct ContextWait {
    cnt: Arc<AtomicUsize>,
}

impl Actor for ContextWait {
    type Context = actix::Context<Self>;
}

struct Ping;

impl Message for Ping {
    type Result = ();
}

impl Handler<Ping> for ContextWait {
    type Result = ();

    fn handle(&mut self, _: Ping, ctx: &mut Self::Context) {
        let cnt = self.cnt.load(Ordering::Relaxed);
        self.cnt.store(cnt + 1, Ordering::Relaxed);

        let fut = sleep(Duration::from_secs(1));
        fut.into_actor(self).wait(ctx);

        System::current().stop();
    }
}

#[test]
fn test_wait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let cnt = Arc::clone(&m);

    let sys = System::new();
    sys.block_on(async move {
        let addr = ContextWait { cnt }.start();
        addr.do_send(Ping);
        addr.do_send(Ping);
        addr.do_send(Ping);
    });

    sys.run().unwrap();

    assert_eq!(m.load(Ordering::Relaxed), 1);
}

#[test]
fn test_message_stream_wait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    let sys = System::new();
    sys.block_on(async move {
        let _addr = ContextWait::create(move |ctx| {
            let (tx, rx) = unbounded_channel();
            let _ = tx.send(Ping);
            let _ = tx.send(Ping);
            let _ = tx.send(Ping);
            let actor = ContextWait { cnt: m2 };

            ctx.add_message_stream(StreamRx { rx });
            actor
        });
    });

    sys.run().unwrap();

    assert_eq!(m.load(Ordering::Relaxed), 1);
}

#[test]
fn test_stream_wait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    let sys = System::new();
    sys.block_on(async move {
        let _addr = ContextWait::create(move |ctx| {
            let (tx, rx) = unbounded_channel();
            let _ = tx.send(Ping);
            let _ = tx.send(Ping);
            let _ = tx.send(Ping);
            let actor = ContextWait { cnt: m2 };
            ctx.add_message_stream(StreamRx { rx });
            actor
        });
    });

    sys.run().unwrap();

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

#[actix::test]
async fn test_nowait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let cnt = Arc::clone(&m);

    actix_rt::spawn(async move {
        let addr = ContextNoWait { cnt }.start();
        addr.do_send(Ping);
        addr.do_send(Ping);
        addr.do_send(Ping);
    });

    sleep(Duration::from_millis(200)).await;

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[actix::test]
async fn test_message_stream_nowait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    actix_rt::spawn(async move {
        let _addr = ContextNoWait::create(move |ctx| {
            let (tx, rx) = unbounded_channel();
            let _ = tx.send(Ping);
            let _ = tx.send(Ping);
            let _ = tx.send(Ping);
            let actor = ContextNoWait { cnt: m2 };
            ctx.add_message_stream(StreamRx { rx });
            actor
        });
    });

    sleep(Duration::from_millis(200)).await;

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[test]
fn test_stream_nowait_context() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    let sys = System::new();
    sys.block_on(async move {
        let _addr = ContextNoWait::create(move |ctx| {
            let (tx, rx) = unbounded_channel();
            let _ = tx.send(Ping);
            let _ = tx.send(Ping);
            let _ = tx.send(Ping);
            let actor = ContextNoWait { cnt: m2 };
            ctx.add_message_stream(StreamRx { rx });
            actor
        });

        actix_rt::spawn(async move {
            sleep(Duration::from_millis(200)).await;
            System::current().stop();
        });
    });

    sys.run().unwrap();

    assert_eq!(m.load(Ordering::Relaxed), 3);
}

#[test]
fn test_notify() {
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    let sys = System::new();
    sys.block_on(async move {
        let addr = ContextNoWait::create(move |ctx| {
            ctx.notify(Ping);
            ctx.notify(Ping);
            ContextNoWait {
                cnt: Arc::clone(&m),
            }
        });
        addr.do_send(Ping);

        actix_rt::spawn(async move {
            sleep(Duration::from_millis(200)).await;
            System::current().stop();
        });
    });

    sys.run().unwrap();

    assert_eq!(m2.load(Ordering::Relaxed), 3);
}

struct ContextHandle {
    h: Arc<AtomicUsize>,
}
impl Actor for ContextHandle {
    type Context = Context<Self>;
}

impl StreamHandler<Ping> for ContextHandle {
    fn handle(&mut self, _: Ping, ctx: &mut Self::Context) {
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

    let sys = System::new();
    sys.block_on(async move {
        let _addr = ContextHandle::create(move |ctx| {
            h2.store(
                ContextHandle::add_stream(once(async { Ping }), ctx).into_usize(),
                Ordering::Relaxed,
            );

            ContextHandle { h: m2 }
        });
    });

    sys.run().unwrap();

    assert_eq!(m.load(Ordering::Relaxed), h.load(Ordering::Relaxed));
}

#[test]
fn test_start_from_context() {
    let h = Arc::new(AtomicUsize::new(0));
    let h2 = Arc::clone(&h);
    let m = Arc::new(AtomicUsize::new(0));
    let m2 = Arc::clone(&m);

    let sys = System::new();
    sys.block_on(async move {
        let _addr = ContextHandle::create(move |ctx| {
            h2.store(
                ctx.add_stream(once(async { Ping })).into_usize(),
                Ordering::Relaxed,
            );
            ContextHandle { h: m2 }
        });
    });

    sys.run().unwrap();

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

impl StreamHandler<CancelPacket> for CancelHandler {
    fn handle(&mut self, _: CancelPacket, ctx: &mut Context<Self>) {
        ctx.cancel_future(self.source);
    }
}

#[test]
fn test_cancel_handler() {
    actix::System::new().block_on(async {
        struct WtfStream {
            interval: actix_rt::time::Interval,
        }

        impl Stream for WtfStream {
            type Item = CancelPacket;

            fn poll_next(
                self: Pin<&mut Self>,
                cx: &mut StdContext<'_>,
            ) -> Poll<Option<Self::Item>> {
                self.get_mut()
                    .interval
                    .poll_tick(cx)
                    .map(|_| Some(CancelPacket))
            }
        }

        CancelHandler::create(|ctx| CancelHandler {
            source: ctx.add_stream(WtfStream {
                interval: interval_at(Instant::now(), Duration::from_millis(1)),
            }),
        });
    });
}

struct CancelLater {
    handle: Option<SpawnHandle>,
}

impl Actor for CancelLater {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        // nothing spawned other than the future to be canceled after completion
        self.handle = Some(ctx.spawn(async {}.into_actor(self)));
    }
}

struct CancelMessage;

impl Message for CancelMessage {
    type Result = ();
}

impl Handler<CancelMessage> for CancelLater {
    type Result = ();

    fn handle(&mut self, _: CancelMessage, ctx: &mut Self::Context) {
        ctx.cancel_future(self.handle.take().unwrap());
    }
}

#[test]
fn test_cancel_completed_with_no_context_item() {
    actix::System::new().block_on(async {
        // first, spawn future that will complete immediately
        let addr = CancelLater { handle: None }.start();

        // then, cancel the future which would already be completed
        actix_rt::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            addr.do_send(CancelMessage);
        });

        // finally, terminate the actor, which shouldn't be blocked unless
        // the actor context ate up CPU time
        actix_rt::spawn(async {
            sleep(Duration::from_millis(200)).await;
            System::current().stop();
        });
    });
}
