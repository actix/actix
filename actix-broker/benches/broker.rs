//! Based on https://github.com/ibraheemdev/matchit/blob/master/benches/bench.rs

use std::{
    future::Future,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
    time::Duration,
};

use actix::{Actor, Context, Handler, Message, System};
use actix_broker::{Broker, BrokerSubscribe, SystemBroker};
use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use tokio::sync::{mpsc, mpsc::Receiver};

#[derive(Clone, Message)]
#[rtype(result = "()")]
struct MessageTest {
    notifier: mpsc::Sender<()>,
    count: Arc<AtomicU16>,
}

#[derive(Default)]
struct ActorTest;

impl Actor for ActorTest {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<SystemBroker, MessageTest>(ctx);
    }
}

impl Handler<MessageTest> for ActorTest {
    type Result = ();
    fn handle(&mut self, msg: MessageTest, _ctx: &mut Self::Context) -> Self::Result {
        let current = msg.count.fetch_sub(1, Ordering::SeqCst) - 1;
        if current == 0 {
            tokio::spawn(async move {
                msg.notifier
                    .send(())
                    .await
                    .expect("The channel should not be closed");
            });
        }
    }
}

fn init_system_runner<F>(fn_: F)
where
    F: Future<Output = ()>,
{
    let system = System::new();
    system.block_on(async {
        fn_.await;
        System::current().stop();
    });
    system.run().expect("Exit Code should be zero");
}

async fn init_actors(num: usize) {
    let mut waiters: Vec<Receiver<()>> = (0..num)
        .map(|_| {
            let addr = ActorTest.start();
            let (tx, rx) = mpsc::channel::<()>(1);
            let message = MessageTest {
                notifier: tx,
                count: Arc::new(AtomicU16::new(1)),
            };
            addr.try_send(message)
                .expect("Actor Mailbox should not bet closed or full");
            rx
        })
        .collect();

    for waiter in waiters.iter_mut() {
        waiter
            .recv()
            .await
            .expect("The channel should not be closed or full");
    }
}
fn issue_async_test(num_actors: usize, num_messages: usize) {
    init_system_runner(async move {
        init_actors(num_actors).await;
        let rxs: Vec<Receiver<()>> = (0..num_messages)
            .map(|_| {
                let (tx, rx) = mpsc::channel::<()>(1);
                let message = MessageTest {
                    notifier: tx,
                    count: Arc::new(AtomicU16::new(num_actors as u16)),
                };
                Broker::<SystemBroker>::issue_async(message);
                rx
            })
            .collect();

        for mut rx in rxs.into_iter() {
            rx.recv()
                .await
                .expect("The channel should not be closed or full");
        }
    });
}

fn broker_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("broker_suite_test");

    let num_actor_list = [10, 25, 100, 1000];
    let num_message_list = [100, 1000, 10000];
    for num_actors in num_actor_list {
        for num_messages in num_message_list {
            let input = (num_actors, num_messages);
            let parameter = format!("Actors: {} - Messages: {}", num_actors, num_messages);
            let total_notifications = num_actors * num_messages;

            let group_ref = group
                .sample_size(10)
                .measurement_time(Duration::from_secs(30))
                .noise_threshold(0.05)
                .sampling_mode(SamplingMode::Flat)
                .throughput(Throughput::Elements(total_notifications as u64));

            group_ref.bench_with_input(
                BenchmarkId::new("issue_async", parameter.as_str()),
                &input,
                |b, &(num_actors, num_messages)| {
                    b.iter(|| issue_async_test(num_actors, num_messages));
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, broker_benches);
criterion_main!(benches);
