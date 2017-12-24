extern crate actix;
#[macro_use] extern crate actix_derive;
extern crate futures;

use actix::{msgs, Actor, Address, Arbiter, Context, Handler, Response, System};
use futures::{future, Future};

#[derive(Message)]
struct Empty;

struct EmptyActor;

impl Actor for EmptyActor {
    type Context = Context<Self>;
}

impl Handler<Empty> for EmptyActor {
    fn handle(&mut self, _message: Empty, _context: &mut Context<Self>) -> Response<Self, Empty> {
        Self::empty()
    }
}

#[test]
fn response_derive_empty() {
    let system = System::new("test");
    let addr: Address<_> = EmptyActor.start();
    let res = addr.call_fut(Empty);
    
    system.handle().spawn(res.then(|res| {
        match res {
            Ok(Ok(result)) => assert!(result == ()),
            _ => panic!("Something went wrong"),
        }
        
        Arbiter::system().send(msgs::SystemExit(0));
        future::result(Ok(()))
    }));

    system.run();
}
