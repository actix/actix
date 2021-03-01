use actix::prelude::*;

struct AsyncMsg;

impl Message for AsyncMsg {
    type Result = usize;
}

struct Msg;

impl Message for Msg {
    type Result = usize;
}

struct MyActor(usize);

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl Handler<AsyncMsg> for MyActor {
    type Result = ResponseAsync<Self, usize>;

    fn handle(&mut self, _: AsyncMsg, ctx: &mut Self::Context) -> Self::Result {
        ResponseAsync::atomic(self, ctx, |act, _| async move {
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

#[actix::test]
async fn test_async_response() {
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
}
