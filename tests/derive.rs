extern crate futures;
#[macro_use]
extern crate actix;
extern crate tokio;

use actix::{Actor, Context, Handler, System};
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
#[cfg_attr(feature = "cargo-clippy", allow(unit_cmp))]
fn response_derive_empty() {
    System::run(|| {
        let addr = EmptyActor.start();
        let res = addr.send(Empty);

        tokio::spawn(res.then(|res| {
            match res {
                Ok(result) => assert!(result == ()),
                _ => panic!("Something went wrong"),
            }

            System::current().stop();
            future::result(Ok(()))
        }));
    });
}

#[derive(Message)]
#[rtype(result = "Result<usize, ()>")]
struct SumResult(usize, usize);

struct SumResultActor;

impl Actor for SumResultActor {
    type Context = Context<Self>;
}

impl Handler<SumResult> for SumResultActor {
    type Result = Result<usize, ()>;

    fn handle(
        &mut self, message: SumResult, _context: &mut Context<Self>,
    ) -> Self::Result {
        Ok(message.0 + message.1)
    }
}

#[test]
pub fn derive_result() {
    System::run(|| {
        let addr = SumResultActor.start();
        let res = addr.send(SumResult(10, 5));

        tokio::spawn(res.then(|res| {
            match res {
                Ok(result) => assert!(result == Ok(10 + 5)),
                _ => panic!("Something went wrong"),
            }

            System::current().stop();
            future::result(Ok(()))
        }));
    });
}

#[derive(Message)]
#[rtype(usize)]
struct SumOne(usize, usize);

struct SumOneActor;

impl Actor for SumOneActor {
    type Context = Context<Self>;
}

impl Handler<SumOne> for SumOneActor {
    type Result = usize;

    fn handle(&mut self, message: SumOne, _context: &mut Context<Self>) -> Self::Result {
        message.0 + message.1
    }
}

#[test]
pub fn response_derive_one() {
    System::run(|| {
        let addr = SumOneActor.start();
        let res = addr.send(SumOne(10, 5));

        tokio::spawn(res.then(|res| {
            match res {
                Ok(result) => assert!(result == 10 + 5),
                _ => panic!("Something went wrong"),
            }

            System::current().stop();
            future::result(Ok(()))
        }));
    });
}
