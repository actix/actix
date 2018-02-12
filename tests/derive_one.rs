extern crate futures;
#[macro_use] extern crate actix;

use actix::prelude::*;
use futures::{future, Future};

#[derive(Message)]
#[rtype(usize)]
struct Sum(usize, usize);

struct SumActor;

impl Actor for SumActor {
    type Context = Context<Self>;
}

impl Handler<Sum> for SumActor {
    type Result = usize;

    fn handle(&mut self, message: Sum, _context: &mut Context<Self>) -> Self::Result {
        message.0 + message.1
    }
}

#[test]
pub fn response_derive_one() {
    let system = System::new("test");
    let addr: Addr<Unsync<_>> = SumActor.start();
    let res = addr.call_fut(Sum(10, 5));
    
    system.handle().spawn(res.then(|res| {
        match res {
            Ok(result) => assert!(result == 10 + 5),
            _ => panic!("Something went wrong"),
        }
        
        Arbiter::system().send(actix::msgs::SystemExit(0));
        future::result(Ok(()))
    }));

    system.run();
}
