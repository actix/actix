extern crate actix;
extern crate actix_broker;

use std::time::Duration;

use actix::{clock::sleep, prelude::*};
use actix_broker::{Broker, BrokerSubscribe, SystemBroker};

#[derive(Clone, Message)]
#[rtype(result = "()")]
struct TestMessage;

#[derive(Default)]
struct TestActor {
    count: u8,
}

impl Actor for TestActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<SystemBroker, TestMessage>(ctx);
    }
}

impl Handler<TestMessage> for TestActor {
    type Result = AtomicResponse<Self, ()>;

    fn handle(&mut self, _msg: TestMessage, _ctx: &mut Self::Context) -> Self::Result {
        let first = self.count == 0;
        let task = async move {
            if first {
                sleep(Duration::from_millis(500)).await;
            }
        }
        .into_actor(self)
        .map(|_, act, _ctx| {
            act.count += 1;
            if act.count == 51 {
                System::current().stop();
            }
        })
        .boxed_local();

        AtomicResponse::new(task)
    }
}

#[test]
fn it_issues_async_on_full_mailbox() {
    let sys = System::new();

    sys.block_on(async {
        let addr = TestActor::default().start();
        sleep(Duration::from_millis(100)).await;
        addr.try_send(TestMessage)
            .expect("Unable to send base message");
        for _ in 0..50 {
            Broker::<SystemBroker>::issue_async(TestMessage);
        }
    });

    sys.run().unwrap();
}
