extern crate actix;
extern crate futures;
extern crate tokio;

#[macro_use]
extern crate actix_derive;

use actix::prelude::*;
use futures::Future;

#[derive(Message)]
struct Toggle;

struct Status;

impl Message for Status {
    type Result = bool;
}

struct MyActor {
    toggle: bool,
}

impl Default for MyActor {
    fn default() -> Self {
        Self { toggle: false }
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
    type Result = bool;

    fn handle(&mut self, _: Status, _ctx: &mut Context<MyActor>) -> Self::Result {
        self.toggle
    }
}

// shipping messages with back pressure
fn main() {
    System::run(|| {
        let my_actor = MyActor::default().start();

        let fut = my_actor.send(Status).map_err(|_| ()).and_then(move |r| {
            println!("INITIAL: {}", r);

            my_actor.send(Toggle).map_err(|_| ()).and_then(move |_| {
                println!("TOGGLE!");

                my_actor.send(Status).map_err(|_| ()).map(|r| {
                    println!("RESULT: {}", r);

                    System::current().stop()
                })
            })
        });

        tokio::spawn(fut);
    });
}
