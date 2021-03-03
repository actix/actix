use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use actix::clock::sleep;
use actix::prelude::*;

struct MyActor {
    timeout: Arc<AtomicBool>,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        async {
            sleep(Duration::from_millis(20)).await;
            System::current().stop();
        }
        .into_actor(self)
        .timeout(Duration::from_millis(1))
        .map(|e, act, _| {
            if e == Err(()) {
                act.timeout.store(true, Ordering::Relaxed);
                System::current().stop();
            }
        })
        .wait(ctx);
    }
}

#[test]
fn test_fut_timeout() {
    let timeout = Arc::new(AtomicBool::new(false));
    let timeout2 = Arc::clone(&timeout);

    let sys = System::new();
    sys.block_on(async {
        let _addr = MyActor { timeout: timeout2 }.start();
    });

    sys.run().unwrap();

    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}

struct MyStreamActor {
    counter: usize,
    timeout: Arc<AtomicBool>,
}

impl Actor for MyStreamActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let mut s = futures_util::stream::FuturesOrdered::new();
        s.push(sleep(Duration::new(0, 5_000_000)));
        s.push(sleep(Duration::new(0, 5_000_000)));

        s.into_actor(self)
            .timeout(Duration::new(0, 1000))
            .then(|res, act, _| {
                // Additional waiting time to test `then` call as well
                async move {
                    sleep(Duration::from_millis(500)).await;
                    res
                }
                .into_actor(act)
            })
            .map(|e, act, _| {
                if let Err(()) = e {
                    act.timeout.store(true, Ordering::Relaxed);
                    System::current().stop();
                }
            })
            .finish()
            .wait(ctx)
    }
}

struct TakeWhileMsg(usize);

impl Message for TakeWhileMsg {
    type Result = ();
}

impl Handler<TakeWhileMsg> for MyStreamActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: TakeWhileMsg, _: &mut Context<Self>) -> Self::Result {
        let num = msg.0;
        Box::pin(
            futures_util::stream::repeat(num)
                .into_actor(self)
                .take_while(move |n, act, ctx| {
                    ctx.spawn(
                        async {
                            actix_rt::task::yield_now().await;
                        }
                        .into_actor(act),
                    );
                    assert_eq!(*n, num);
                    assert!(act.counter < 10);
                    act.counter += 1;
                    futures_util::future::ready(act.counter < 10)
                })
                .finish(),
        )
    }
}

struct SkipWhileMsg(usize);

impl Message for SkipWhileMsg {
    type Result = ();
}

impl Handler<SkipWhileMsg> for MyStreamActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: SkipWhileMsg, _: &mut Context<Self>) -> Self::Result {
        let num = msg.0;
        Box::pin(
            futures_util::stream::repeat(num)
                .into_actor(self)
                .take_while(|_, act, _| {
                    let cond = act.counter < 10;
                    act.counter += 1;
                    futures_util::future::ready(cond)
                })
                .skip_while(move |n, act, ctx| {
                    let fut = async {
                        actix_rt::task::yield_now().await;
                    }
                    .into_actor(act);
                    ctx.spawn(fut);
                    assert_eq!(*n, num);
                    act.counter -= 1;
                    futures_util::future::ready(act.counter > 0)
                })
                .finish(),
        )
    }
}

struct CollectMsg(usize);

impl Message for CollectMsg {
    type Result = Vec<usize>;
}

impl Handler<CollectMsg> for MyStreamActor {
    type Result = ResponseActFuture<Self, Vec<usize>>;

    fn handle(&mut self, msg: CollectMsg, _: &mut Context<Self>) -> Self::Result {
        let num = msg.0;
        Box::pin(
            futures_util::stream::repeat(num)
                .into_actor(self)
                .take_while(|_, act, _| {
                    let cond = act.counter < 5;
                    act.counter += 1;
                    futures_util::future::ready(cond)
                })
                .map(|_, act, _| act.counter)
                .collect(),
        )
    }
}

#[test]
fn test_stream_timeout() {
    let timeout = Arc::new(AtomicBool::new(false));
    let timeout2 = Arc::clone(&timeout);

    let sys = System::new();
    sys.block_on(async {
        let _addr = MyStreamActor {
            timeout: timeout2,
            counter: 0,
        }
        .start();
    });
    sys.run().unwrap();

    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}

#[test]
fn test_stream_take_while() {
    System::new().block_on(async {
        let addr = MyStreamActor {
            timeout: Arc::new(AtomicBool::new(false)),
            counter: 0,
        }
        .start();
        addr.send(TakeWhileMsg(5)).await.unwrap();
    })
}

#[test]
fn test_stream_skip_while() {
    System::new().block_on(async {
        let addr = MyStreamActor {
            timeout: Arc::new(AtomicBool::new(false)),
            counter: 0,
        }
        .start();
        addr.send(SkipWhileMsg(5)).await.unwrap();
    })
}

#[test]
fn test_stream_collect() {
    System::new().block_on(async {
        let addr = MyStreamActor {
            timeout: Arc::new(AtomicBool::new(false)),
            counter: 0,
        }
        .start();
        let res = addr.send(CollectMsg(3)).await.unwrap();

        assert_eq!(res, vec![1, 2, 3, 4, 5]);
    })
}
