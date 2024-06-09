use std::time::Duration;

use actix::prelude::*;
use actix_broker::{BrokerIssue, BrokerSubscribe, SystemBroker};

struct ActorOne;
struct ActorTwo;
struct ActorThree;

type BrokerType = SystemBroker;

impl Actor for ActorOne {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("ActorOne started");
        self.subscribe_sync::<BrokerType, MessageTwo>(ctx);
        self.issue_async::<BrokerType, _>(MessageOne("hello".to_string()));
        let _ = ctx.run_later(Duration::from_millis(50), |_, _| {
            System::current().stop();
            println!("Bye!");
        });
    }
}

impl Handler<MessageTwo> for ActorOne {
    type Result = ();

    fn handle(&mut self, msg: MessageTwo, _ctx: &mut Self::Context) {
        println!("ActorOne Received: {:?}", msg.0);
    }
}

impl Actor for ActorTwo {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("ActorTwo started");
        self.subscribe_sync::<BrokerType, MessageOne>(ctx);
    }
}

impl Handler<MessageOne> for ActorTwo {
    type Result = ();

    fn handle(&mut self, msg: MessageOne, _ctx: &mut Self::Context) {
        println!("ActorTwo Received: {:?}", msg.0);
        self.issue_async::<BrokerType, _>(MessageTwo(0));
    }
}

impl Actor for ActorThree {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("Actor 3 started");
        self.subscribe_async::<BrokerType, MessageOne>(ctx);
    }
}

impl Handler<MessageOne> for ActorThree {
    type Result = ();

    fn handle(&mut self, msg: MessageOne, _ctx: &mut Self::Context) {
        println!("ActorThree Received: {:?}", msg.0);
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
    println!("Starting");
    let sys = System::new();

    sys.block_on(async {
        ActorTwo.start();
        ActorThree.start();
        ActorOne.start();
    });
    sys.run().unwrap();
    println!("Done");
}
