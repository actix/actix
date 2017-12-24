extern crate actix;
#[macro_use] extern crate actix_derive;
extern crate futures;

use actix::{msgs, Actor, Address, Arbiter, Context, Handler, Response, System};
use futures::{future, Future};

#[derive(Message)]
#[rtype(usize)]
struct Sum(usize, usize);

struct SumActor;

impl Actor for SumActor {
    type Context = Context<Self>;
}

impl Handler<Sum> for SumActor {
    fn handle(&mut self, message: Sum, _context: &mut Context<Self>) -> Response<Self, Sum> {
        Self::reply(message.0 + message.1)
    }
}

#[test]
pub fn response_derive_one() {
    let system = System::new("test");
    let addr: Address<_> = SumActor.start();
    let res = addr.call_fut(Sum(10, 5));
    
    system.handle().spawn(res.then(|res| {
        match res {
            Ok(Ok(result)) => assert!(result == 10 + 5),
            _ => panic!("Something went wrong"),
        }
        
        Arbiter::system().send(msgs::SystemExit(0));
        future::result(Ok(()))
    }));

    system.run();
}
