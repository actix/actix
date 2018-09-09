extern crate actix;
extern crate futures;
extern crate tokio;

use actix::prelude::*;
use futures::Future;

struct Toggle;

impl Message for Toggle {
    type Result = ();
}

struct Status;

impl Message for Status {
    type Result = Result<bool, ()>;
}

struct MyActor {
    toggle: bool
}

impl MyActor {
    pub fn new() -> Self {
        MyActor {
            toggle: false,
        }
    }
}

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl Handler<Toggle> for MyActor {
    type Result = ();

    fn handle(&mut self, _: Toggle, _ctx: &mut Context<MyActor>) {
        self.toggle = !self.toggle;
    }
}

impl Handler<Status> for MyActor {
    type Result = Result<bool, ()>;

    fn handle(&mut self, _: Status, _ctx: &mut Context<MyActor>) -> Self::Result {
        Ok(self.toggle)
    }
}



/*
 * Test sending messages in order to the actor.
 * This relies on future chaining so that once message A
 * is complete, we are able to check the content for message B
 */
#[test]
fn test_order_msg() {
    System::run(|| {
        let myactor = MyActor::new();
        let myactor_addr = myactor.start();

        let fut = myactor_addr.send(Toggle)
            .map_err(|_| ())
            .and_then(move |_res| {
                myactor_addr.send(Status)
                    .map_err(|_| ())
                    .map(|r| {
                        assert_eq!(r, Ok(true));
                        System::current().stop();
                    })
            });

        tokio::spawn(fut);
    });
}

