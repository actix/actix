use actix::prelude::*;

#[derive(MessageResponse)]
struct Added(usize);

#[derive(Message)]
#[rtype(result = "Added")]
struct Sum(usize, usize);

#[derive(Message)]
#[rtype(Added)]
struct SumBareType(usize, usize);

#[derive(Message)]
#[rtype("()")]
struct UnitReturnInString;

#[derive(Default)]
struct Adder;

impl Actor for Adder {
    type Context = Context<Self>;
}

impl Handler<Sum> for Adder {
    type Result = <Sum as actix::Message>::Result;
    fn handle(&mut self, msg: Sum, _: &mut Self::Context) -> Added {
        Added(msg.0 + msg.1)
    }
}

impl Handler<SumBareType> for Adder {
    type Result = <SumBareType as actix::Message>::Result;
    fn handle(&mut self, msg: SumBareType, _: &mut Self::Context) -> Added {
        Added(msg.0 + msg.1)
    }
}

#[actix::main]
async fn main() {
    let addr = Adder::start_default();

    let res = addr.send(Sum(3, 5)).await.unwrap();
    assert_eq!(res.0, 8);

    let res = addr.send(SumBareType(4, 2)).await.unwrap();
    assert_eq!(res.0, 6);
}
