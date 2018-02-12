extern crate futures;
#[macro_use] extern crate actix;

use actix::{msgs, Actor, Addr, Arbiter, Context, Handler, System, Unsync};
use futures::{future, Future};

#[derive(Message)]
struct Empty;

struct EmptyActor;

impl Actor for EmptyActor {
    type Context = Context<Self>;
}

impl Handler<Empty> for EmptyActor {
    type Result = ();

    fn handle(&mut self, _message: Empty, _context: &mut Context<Self>) {}
}

#[test]
#[cfg_attr(feature="cargo-clippy", allow(unit_cmp))]
fn response_derive_empty() {
    let system = System::new("test");
    let addr: Addr<Unsync<_>> = EmptyActor.start();
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
