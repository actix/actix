use std::pin::Pin;

use futures::{future::ready, StreamExt};

use actix::prelude::*;
use actix::{AsyncHandler, AsyncMessage, TempRef};
use std::time::{Duration, Instant};
use tokio::time::delay_for;

#[derive(Debug, Clone, Copy)]
struct Num(usize);

impl Message for Num {
    type Result = usize;
}

impl AsyncMessage for Num {}

struct MyActor {
    count: usize,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;
}

impl AsyncHandler<Num> for MyActor {
    type Result = Pin<Box<dyn Future<Output = <Num as Message>::Result>>>;

    fn handle(
        mut this: TempRef<Self>,
        msg: Num,
        _ctx: TempRef<Self::Context>,
    ) -> Self::Result {
        let res = tokio::task::spawn(async move {
            delay_for(Duration::from_millis(msg.0 as u64)).await;
            this.as_mut().count += msg.0;
            this.as_ref().count
        });

        Box::pin(async move { res.await.unwrap() })
    }
}

#[actix_rt::test]
async fn test_async_actor() {
    let items = vec![Num(7), Num(6), Num(5), Num(4), Num(3), Num(2), Num(1)];

    let addr = MyActor { count: 0 }.start();

    let fut = futures::stream::iter(items.clone())
        .map(move |i| addr.send(i))
        .buffer_unordered(7)
        .fold(vec![], |mut acc, res| {
            acc.push(res.unwrap());
            ready(acc)
        });

    let start = Instant::now();

    let result = fut.await;

    let took_ms = Instant::now().duration_since(start).as_millis();

    assert_eq!(result, vec![7, 13, 18, 22, 25, 27, 28]);
    assert!(took_ms >= 28)
}
