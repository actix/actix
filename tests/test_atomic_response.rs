use actix::clock::delay_for;
use actix::prelude::*;
use futures_util::stream::StreamExt;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Message)]
#[rtype(result = "usize")]
struct Num(usize);

struct MyActor(usize);

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl Handler<Num> for MyActor {
    type Result = AtomicResponse<Self, usize>;

    fn handle(&mut self, msg: Num, _: &mut Self::Context) -> Self::Result {
        AtomicResponse::new(Box::pin(
            delay_for(Duration::from_millis(msg.0 as u64))
                .into_actor(self)
                .map(move |_res, this, _| {
                    this.0 += msg.0;
                    this.0
                }),
        ))
    }
}

#[actix_rt::test]
async fn test_atomic_response() {
    let items = vec![Num(7), Num(6), Num(5), Num(4), Num(3), Num(2), Num(1)];
    let addr = MyActor(0).start();
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
}
