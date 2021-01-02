use actix::prelude::*;
use actix_broker::{BrokerIssue, BrokerSubscribe, SystemBroker};

struct ActorOne;
struct ActorTwo;
struct ActorThree;

type BrokerType = SystemBroker;

impl Actor for ActorOne {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_sync::<BrokerType, MessageTwo>(ctx);
        self.issue_async::<BrokerType, _>(MessageOne("hello".to_string()));
    }
}

impl Handler<MessageTwo> for ActorOne {
    type Result = ();

    fn handle(&mut self, msg: MessageTwo, _ctx: &mut Self::Context) {
        println!("ActorOne Received: {:?}", msg);
    }
}

impl Actor for ActorTwo {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<BrokerType, MessageOne>(ctx);
    }
}

impl Handler<MessageOne> for ActorTwo {
    type Result = ();

    fn handle(&mut self, msg: MessageOne, _ctx: &mut Self::Context) {
        println!("ActorTwo Received: {:?}", msg);
        self.issue_async::<BrokerType, _>(MessageTwo(0));
    }
}

impl Actor for ActorThree {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<BrokerType, MessageOne>(ctx);
    }
}

impl Handler<MessageOne> for ActorThree {
    type Result = ();

    fn handle(&mut self, msg: MessageOne, _ctx: &mut Self::Context) {
        println!("ActorThree Received: {:?}", msg);
        self.issue_async::<BrokerType, _>(MessageTwo(1));
    }
}

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
struct MessageOne(String);

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
struct MessageTwo(u8);

fn main() {
    let _ = System::run(|| {
        ActorTwo.start();
        ActorThree.start();
        ActorOne.start();
    });
}
