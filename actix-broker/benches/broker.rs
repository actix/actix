//! Based on https://github.com/ibraheemdev/matchit/blob/master/benches/bench.rs

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use actix_web::rt::SystemRunner;
use criterion::{Criterion, criterion_group, criterion_main};
use log::info;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::Receiver;

use actix::{Actor, Arbiter, AtomicResponse, Context, Handler, Message, System};
use actix::clock::sleep;
use actix_broker::{Broker, BrokerSubscribe, SystemBroker};

#[derive(Clone, Message)]
#[rtype(result = "()")]
struct TestMessage {
    notifier: mpsc::Sender<()>,
    count: Arc<AtomicU16>,
}

#[derive(Default)]
struct TestActor;

impl Actor for TestActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<SystemBroker, TestMessage>(ctx);
    }
}

impl Handler<TestMessage> for TestActor {
    type Result = ();
    fn handle(&mut self, msg: TestMessage, _ctx: &mut Self::Context) -> Self::Result {
        let current = msg.count.fetch_sub(1, Ordering::SeqCst) - 1;
        if current == 0 {
            tokio::spawn(async move {
                msg.notifier.send(()).await.unwrap();
            });
        }
    }
}

async fn init_actors(num: usize) {
    let mut waiters: Vec<Receiver<()>> = (0..num).into_iter().map(|actor| {
        let addr = TestActor::default().start();
        let (tx, rx) = mpsc::channel::<()>(1);
        addr.try_send(TestMessage {
            notifier: tx,
            count: Arc::new(AtomicU16::new(1)),
        })
            .expect("Unable to send base message");
        rx
    }).collect();

    for waiter in waiters.iter_mut() {
        waiter.recv().await.unwrap();
    }
}


fn compare_brokers(c: &mut Criterion) {
    let mut group = c.benchmark_group("Compare Brokers");

    let issue_async_fn = |num_actors: usize, num_msgs: usize| {
        return move || {
            let sys = System::new();
            sys.block_on(async {
                init_actors(num_actors).await;
                let mut waiters = vec![];
                (0..num_msgs).for_each(|_| {
                    let (tx, rx) = mpsc::channel::<()>(1);
                    waiters.push(rx);
                    let message = TestMessage {
                        notifier: tx,
                        count: Arc::new(AtomicU16::new(num_actors as u16)),
                    };
                    Broker::<SystemBroker>::issue_async(message);
                });

                for waiter in waiters.iter_mut() {
                    waiter.recv().await.unwrap();
                }

                System::current().stop();
            });
            sys.run().unwrap();
        };
    };

    group.warm_up_time(Duration::from_nanos(1))
        .bench_function("issue_async", |b| {
            b.iter(issue_async_fn(100, 1000));
        });

    group.finish();
}

criterion_group!(benches, compare_brokers);
criterion_main!(benches);
