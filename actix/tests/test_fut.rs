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
    timeout: Arc<AtomicBool>,
}

impl Actor for MyStreamActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let mut s = futures_util::stream::FuturesOrdered::new();
        s.push(sleep(Duration::from_millis(20)));
        s.push(sleep(Duration::from_millis(20)));

        s.into_actor(self)
            .timeout(Duration::from_millis(1))
            .then(|res, act, _| {
                // Additional waiting time to test `then` call as well
                async move {
                    sleep(Duration::from_millis(20)).await;
                    res
                }
                .into_actor(act)
            })
            .map(|res, act, _| {
                assert!(
                    res.is_err(),
                    "MyStreamActor should return error when timed out"
                );
                act.timeout.store(true, Ordering::Relaxed);
                System::current().stop();
            })
            .finish()
            .wait(ctx)
    }
}

struct MyStreamActor2 {
    counter: usize,
}

impl Actor for MyStreamActor2 {
    type Context = actix::Context<Self>;
}

struct TakeWhileMsg(usize);

impl Message for TakeWhileMsg {
    type Result = ();
}

impl Handler<TakeWhileMsg> for MyStreamActor2 {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: TakeWhileMsg, _: &mut Context<Self>) -> Self::Result {
        let num = msg.0;
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
            .finish()
            .boxed_local()
    }
}

struct SkipWhileMsg(usize);

impl Message for SkipWhileMsg {
    type Result = ();
}

impl Handler<SkipWhileMsg> for MyStreamActor2 {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: SkipWhileMsg, _: &mut Context<Self>) -> Self::Result {
        let num = msg.0;
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
            .finish()
            .boxed_local()
    }
}

struct CollectMsg(usize);

impl Message for CollectMsg {
    type Result = Vec<usize>;
}

impl Handler<CollectMsg> for MyStreamActor2 {
    type Result = ResponseActFuture<Self, Vec<usize>>;

    fn handle(&mut self, msg: CollectMsg, _: &mut Context<Self>) -> Self::Result {
        let num = msg.0;
        futures_util::stream::repeat(num)
            .into_actor(self)
            .take_while(|_, act, _| {
                let cond = act.counter < 5;
                act.counter += 1;
                futures_util::future::ready(cond)
            })
            .map(|_, act, _| act.counter)
            .collect()
            .boxed_local()
    }
}

struct TryFutureMsg(usize);

impl Message for TryFutureMsg {
    type Result = Result<isize, u32>;
}

impl Handler<TryFutureMsg> for MyStreamActor2 {
    type Result = ResponseActFuture<Self, Result<isize, u32>>;

    fn handle(&mut self, msg: TryFutureMsg, _: &mut Context<Self>) -> Self::Result {
        let num = msg.0;
        async move {
            assert_eq!(num, 5);
            Ok::<usize, usize>(num * 2)
        }
        .into_actor(self)
        .map_ok(|res, _, _| {
            assert_eq!(10usize, res);
            res as isize
        })
        .and_then(|res, _, _| {
            assert_eq!(10isize, res);
            fut::err::<isize, usize>(996)
        })
        .map_err(|e, _, _| {
            assert_eq!(996usize, e);
            e as u32
        })
        .boxed_local()
    }
}

#[test]
fn test_stream_timeout() {
    let timeout = Arc::new(AtomicBool::new(false));
    let timeout2 = Arc::clone(&timeout);

    let sys = System::new();
    sys.block_on(async {
        let _addr = MyStreamActor { timeout: timeout2 }.start();
    });
    sys.run().unwrap();

    assert!(timeout.load(Ordering::Relaxed), "Not timeout");
}

#[test]
fn test_stream_take_while() {
    System::new().block_on(async {
        let addr = MyStreamActor2 { counter: 0 }.start();
        addr.send(TakeWhileMsg(5)).await.unwrap();
    })
}

#[test]
fn test_stream_skip_while() {
    System::new().block_on(async {
        let addr = MyStreamActor2 { counter: 0 }.start();
        addr.send(SkipWhileMsg(5)).await.unwrap();
    })
}

#[test]
fn test_stream_collect() {
    System::new().block_on(async {
        let addr = MyStreamActor2 { counter: 0 }.start();
        let res = addr.send(CollectMsg(3)).await.unwrap();

        assert_eq!(res, vec![1, 2, 3, 4, 5]);
    })
}

#[test]
fn test_try_future() {
    System::new().block_on(async {
        let addr = MyStreamActor2 { counter: 0 }.start();
        let res = addr.send(TryFutureMsg(5)).await.unwrap();
        assert_eq!(res.err().unwrap(), 996u32);
    })
}
