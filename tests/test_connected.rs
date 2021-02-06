use std::time::Duration;

use actix::prelude::*;
use actix_rt::time::sleep;

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

#[actix::test]
async fn test_connected() {
    actix_rt::spawn(async move {
        let addr = MyActor::start(MyActor);
        sleep(Duration::from_millis(350)).await;
        drop(addr);
    });
}
