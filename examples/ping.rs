use actix::prelude::*;
use actix_rt::spawn;
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

/// Declare actor and its context
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

fn main() -> std::io::Result<()> {
    // start system, this is required step
    System::run(|| {
        // start new actor
        let addr = MyActor { count: 10 }.start();


        spawn(async move {
            // send message and get future for result, then await it
            match addr.send(Ping(10)).await {
                Ok(res) => {
                    println!("RESULT: {}", res == 20);
                }
                Err(e) => {
                    println!("FAILURE: {}",e);
                }
            }
            System::current().stop();

        });
    })
}
