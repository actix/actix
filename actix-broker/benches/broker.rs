//! Based on https://github.com/ibraheemdev/matchit/blob/master/benches/bench.rs

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use actix_web::rt::SystemRunner;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, SamplingMode, Throughput};
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
                msg.notifier.send(()).await.expect("");
            });
        }
    }
}

fn test_suite<F>(runner: F)
where
    F: Future<Output=()>,
{
    let system = System::new();
    system.block_on(async {
        runner.await;
        System::current().stop();
    });
    system.run().expect("");
}


// Generic over what type of Actor is
async fn init_actors(num: usize) {
    let mut waiters: Vec<Receiver<()>> = (0..num).into_iter().map(|_| {
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
        waiter.recv().await.expect("");
    }
}
fn issue_async_suite(num_actors: usize, num_messages: usize) {
    test_suite(async move {
        init_actors(num_actors).await;
        let rxs: Vec<Receiver<()>> = (0..num_messages).map(|_| {
            let (tx, rx) = mpsc::channel::<()>(1);
            let message = TestMessage {
                notifier: tx,
                count: Arc::new(AtomicU16::new(num_actors as u16)),
            };
            Broker::<SystemBroker>::issue_async(message);
            rx
        }).collect();

        for mut rx in rxs.into_iter() {
            rx.recv().await.expect("");
        }
    });
}


fn broker_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("Brokers Suite Test");

    let num_actor_list = [10, 25, 100, 1000];
    let num_message_list = [100, 1000, 10000];
    for num_actors in num_actor_list {
        for num_messages in num_message_list {
            let input = (num_actors, num_messages);
            let parameter = format!("Actors: {} - Messages: {}", num_actors, num_messages);
            let total_notifications = num_actors * num_messages;
            group
                .sample_size(10)
                .sampling_mode(SamplingMode::Flat)
                .throughput(Throughput::Elements(total_notifications as u64))
                .bench_with_input(BenchmarkId::new("issue_async", parameter), &input, |b, (i, j)| {
                    b.iter(|| issue_async_suite(*i, *j));
                });
        }
    }

    group.finish();
}

criterion_group!(benches, broker_benches);
criterion_main!(benches);
