use actix::prelude::*;
use std::time::Duration;
use tokio::time::delay_for;

struct MyActor;

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_millis(100), |_this, ctx| {
            if !ctx.connected() {
                ctx.stop();
            }
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        System::current().stop()
    }
}

#[actix_rt::test]
async fn test_connected() {
    Arbiter::spawn(async move {
        let addr = MyActor::start(MyActor);
        delay_for(Duration::from_millis(350)).await;
        drop(addr);
    });
}
