use std::time::{Duration, Instant};

use actix::clock::sleep;
use actix::prelude::*;
use futures_util::stream::StreamExt;

struct AsyncMsg;

impl Message for AsyncMsg {
    type Result = usize;
}

struct Msg;

impl Message for Msg {
    type Result = usize;
}

#[derive(Clone, Copy)]
struct Num(usize);

impl Message for Num {
    type Result = usize;
}

impl Handler<Num> for MyActor {
    type Result = AsyncResponse<Self, usize>;

    fn handle(&mut self, msg: Num, ctx: &mut Self::Context) -> Self::Result {
        AsyncResponse::atomic(self, ctx, move |act, _| async move {
            sleep(Duration::from_millis(msg.0 as u64)).await;
            act.0 += msg.0;
            act.0
        })
    }
}

struct MyActor(usize);

impl Actor for MyActor {
    type Context = Context<Self>;

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        if self.0 == 65535 {
            self.0 += 1;
            Running::Continue
        } else {
            Running::Stop
        }
    }
}

impl Handler<AsyncMsg> for MyActor {
    type Result = AsyncResponse<Self, usize>;

    fn handle(&mut self, _: AsyncMsg, ctx: &mut Self::Context) -> Self::Result {
        AsyncResponse::atomic(self, ctx, |act, _| async move {
            for _ in 0..1000 {
                // yield every step to give other task chance to run.
                // this would test if we have exclusive access to the actor state.
                act.0 += 1;
                actix_rt::task::yield_now().await;
            }

            let res = act.0;

            for _ in 0..1000 {
                act.0 -= 1;
                actix_rt::task::yield_now().await;
            }

            res
        })
    }
}

impl Handler<Msg> for MyActor {
    type Result = usize;

    fn handle(&mut self, _: Msg, _: &mut Self::Context) -> Self::Result {
        self.0
    }
}

struct StopMsg {
    self_yield: bool,
}

impl Message for StopMsg {
    type Result = usize;
}

impl Handler<StopMsg> for MyActor {
    type Result = AsyncResponse<Self, usize>;

    fn handle(&mut self, msg: StopMsg, ctx: &mut Self::Context) -> Self::Result {
        AsyncResponse::atomic(self, ctx, |act, ctx| async move {
            act.0 = 65535;
            ctx.stop();

            // yield to other task before resove the async block.
            // this is to test self context stop followed by async tasks.
            if msg.self_yield {
                actix_rt::task::yield_now().await;
            }

            act.0
        })
    }
}

struct PauseMsg;

impl Message for PauseMsg {
    type Result = ();
}

impl Handler<PauseMsg> for MyActor {
    type Result = AsyncResponse<Self, ()>;

    fn handle(&mut self, _: PauseMsg, ctx: &mut Self::Context) -> Self::Result {
        AsyncResponse::atomic(self, ctx, |act, _| async move {
            // actor state would keep consistant between await point.
            let state = act.0;
            actix_rt::time::sleep(std::time::Duration::from_secs(3)).await;
            assert_eq!(act.0, state);
        })
    }
}

#[test]
fn test_async_response() {
    actix::System::new().block_on(async {
        let addr = MyActor::start(MyActor(0));

        let items = vec![Num(7), Num(6), Num(5), Num(4), Num(3), Num(2), Num(1)];
        let fut = futures_util::stream::iter(items)
            .map(|i| addr.send(i))
            .buffer_unordered(7)
            .fold(Vec::default(), |mut acc, res| {
                acc.push(res.unwrap());
                async { acc }
            });

        let start = Instant::now();
        let result = fut.await;

        assert!(Instant::now().duration_since(start).as_millis() >= 28);
        assert_eq!(result, vec![7, 13, 18, 22, 25, 27, 28]);

        let addr = MyActor::start(MyActor(0));

        let msgs = (0..2000)
            .map(|_| async {
                let res = addr.send(Msg).await.unwrap();
                actix_rt::task::yield_now().await;
                assert_eq!(res, 0);
            })
            .collect::<Vec<_>>();

        let addr1 = addr.clone();

        let task = actix_rt::spawn(async move {
            let res = addr1.send(AsyncMsg).await.unwrap();
            assert_eq!(res, 1000);
        });

        for msg in msgs {
            msg.await;
        }

        let _ = task.await.unwrap();
    })
}

#[test]
fn test_stop_context_with_msessage() {
    actix::System::new().block_on(async {
        let addr = MyActor::start(MyActor(0));

        let addr1 = addr.clone();
        let handle = actix_rt::spawn(async move {
            let _ = addr1.send(PauseMsg).await;
        });

        // extra message that try to stop the context would only be able to
        // run after PauseMsg is done handling.
        let res = addr.send(StopMsg { self_yield: false }).await.unwrap();
        assert_eq!(65535, res);

        let _ = handle.await;
    })
}

#[test]
fn test_self_context_stop() {
    actix::System::new().block_on(async {
        let addr = MyActor::start(MyActor(0));

        let res = addr.send(StopMsg { self_yield: true }).await;

        // Handler stop the context and await on async task afterward would fail.
        // TODO: This is not a good beavior but an expected one for now.
        assert!(res.is_err());
    })
}
