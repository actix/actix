use std::ops::Mul;

use actix::prelude::*;
use actix_derive::{Message, MessageResponse};

#[derive(Message)]
#[rtype(result = "()")]
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
#[allow(clippy::unit_cmp)]
fn response_derive_empty() {
    System::run(|| {
        let addr = EmptyActor.start();
        let res = addr.send(Empty);

        actix_rt::spawn(async move {
            match res.await {
                Ok(result) => assert!(result == ()),
                _ => panic!("Something went wrong"),
            }

            System::current().stop();
        });
    })
    .unwrap();
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
        &mut self,
        message: SumResult,
        _context: &mut Context<Self>,
    ) -> Self::Result {
        Ok(message.0 + message.1)
    }
}

#[test]
pub fn derive_result() {
    System::run(|| {
        let addr = SumResultActor.start();
        let res = addr.send(SumResult(10, 5));

        actix_rt::spawn(async move {
            match res.await {
                Ok(result) => assert!(result == Ok(10 + 5)),
                _ => panic!("Something went wrong"),
            }

            System::current().stop();
        });
    })
    .unwrap();
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

        actix_rt::spawn(async move {
            match res.await {
                Ok(result) => assert!(result == 10 + 5),
                _ => panic!("Something went wrong"),
            }

            System::current().stop();
        });
    })
    .unwrap();
}

#[derive(MessageResponse, PartialEq)]
struct MulRes(usize);

#[derive(Message)]
#[rtype(MulRes)]
struct MulOne(usize, usize);

struct MulOneActor;

impl Actor for MulOneActor {
    type Context = Context<Self>;
}

impl Handler<MulOne> for MulOneActor {
    type Result = MulRes;

    fn handle(&mut self, message: MulOne, _context: &mut Context<Self>) -> Self::Result {
        MulRes(message.0 * message.1)
    }
}

#[test]
pub fn derive_response_one() {
    System::run(|| {
        let addr = MulOneActor.start();
        let res = addr.send(MulOne(10, 5));

        actix_rt::spawn(async move {
            match res.await {
                Ok(result) => assert!(result == MulRes(10 * 5)),
                _ => panic!("Something went wrong"),
            }

            System::current().stop();
        });
    })
    .unwrap();
}

#[derive(MessageResponse, PartialEq)]
struct MulAny<T: 'static + Mul>(T);

#[derive(Message)]
#[rtype(result = "MulAny<usize>")]
struct MulAnyOne(usize, usize);

struct MulAnyOneActor;

impl Actor for MulAnyOneActor {
    type Context = Context<Self>;
}

impl Handler<MulAnyOne> for MulAnyOneActor {
    type Result = MulAny<usize>;

    fn handle(
        &mut self,
        message: MulAnyOne,
        _context: &mut Context<Self>,
    ) -> Self::Result {
        MulAny(message.0 * message.1)
    }
}

#[test]
pub fn derive_response_two() {
    System::run(|| {
        let addr = MulAnyOneActor.start();
        let res = addr.send(MulAnyOne(10, 5));

        actix_rt::spawn(async move {
            match res.await {
                Ok(result) => assert!(result == MulAny(10 * 5)),
                _ => panic!("Something went wrong"),
            }

            System::current().stop();
        });
    })
    .unwrap();
}
