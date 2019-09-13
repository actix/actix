use actix::prelude::*;
use std::time::Duration;
use tokio_timer::sleep;

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

#[test]
fn test_connected() {
    System::run(move || {
        Arbiter::spawn_fn(move || {
            let addr = MyActor::start(MyActor);
            async {
                sleep(Duration::from_millis(350)).await;
                drop(addr);
            }
        });
    })
    .unwrap();
}
