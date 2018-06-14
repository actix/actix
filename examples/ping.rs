extern crate actix;
extern crate futures;
extern crate tokio;

use actix::prelude::*;
use futures::Future;

/// Define `Ping` message
struct Ping(usize);

impl Message for Ping {
    type Result = usize;
}

/// Actor
struct MyActor {
    count: usize,
}

/// Declare actor and it's context
impl Actor for MyActor {
    type Context = Context<Self>;
}

/// Handler for `Ping` message
impl Handler<Ping> for MyActor {
    type Result = usize;

    fn handle(&mut self, msg: Ping, _: &mut Context<Self>) -> Self::Result {
        self.count += msg.0;
        self.count
    }
}

fn main() {
    // start system, this is required step
    System::run(|| {
        // start new actor
        let addr = MyActor { count: 10 }.start();

        // send message and get future for result
        let res = addr.send(Ping(10));

        // handle() returns tokio handle
        tokio::spawn(
            res.map(|res| {
                println!("RESULT: {}", res == 20);

                // stop system and exit
                System::current().stop();
            }).map_err(|_| ()),
        );
    });
}
