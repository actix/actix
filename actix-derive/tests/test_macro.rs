use actix::{Actor, Context, Handler, System};
use actix_derive::{Message, MessageResponse};

#[derive(MessageResponse)]
struct Added(usize);

#[derive(MessageResponse)]
struct Subtracted(usize);

#[derive(Message)]
#[rtype(result = "Added")]
struct Sum(usize, usize);

#[derive(Message)]
#[rtype(result = Subtracted)]
struct Difference(usize, usize);

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

impl Handler<Difference> for Adder {
    type Result = <Difference as actix::Message>::Result;
    fn handle(&mut self, msg: Difference, _: &mut Self::Context) -> Subtracted {
        Subtracted(msg.0 - msg.1)
    }
}

#[test]
fn test_message() {
    System::new().block_on(async {
        let addr = Adder::start_default();
        let res = addr.send(Sum(3, 5)).await.unwrap();
        assert_eq!(res.0, 8);
        let res = addr.send(Difference(5, 3)).await.unwrap();
        assert_eq!(res.0, 2);
    });
}
