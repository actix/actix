//! WeakRecipient example
//!
//! With a weak recipient you can register any client actor's addrs with a
//! service and still get cleaned up correctly when the client actor is
//! stopped. In contrast to a `WeakAddr` the Service is not bound to know
//! exactly which type of client actor is registering itself.

use actix::{prelude::*, WeakRecipient};
use actix::{Actor, Context};

use std::time::{Duration, Instant};

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct TimePing(Instant);

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterForTime(pub WeakRecipient<TimePing>);

#[derive(Debug)]
pub struct TimeService {
    clients: Vec<WeakRecipient<TimePing>>,
}

impl TimeService {
    fn send_tick(&mut self, _ctx: &mut Context<Self>) {
        for client in self.clients.iter() {
            if let Some(client) = client.upgrade() {
                // `Recipient::do_send` seems inconsistent with `Addr::do_send`
                client.do_send(TimePing(Instant::now())).unwrap();
                println!("⏰ sent ping to client {:?}", client);
            } else {
                println!("⏰ client can no longer be upgraded");
            }
        }

        // gc
        self.clients = self
            .clients
            .drain(..)
            .filter(|c| c.upgrade().is_some())
            .collect();
        println!("⏰ service has {} clients", self.clients.len());
    }
}

impl Actor for TimeService {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("⏰ starting TimeService");
        ctx.run_interval(Duration::from_millis(1_000), Self::send_tick);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        println!("⏰ stopping TimeService");
        Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("⏰ stopped TimeService");
    }
}

impl Handler<RegisterForTime> for TimeService {
    type Result = ();

    fn handle(
        &mut self,
        RegisterForTime(client): RegisterForTime,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        println!("⏰ received registration");
        self.clients.push(client);
    }
}

impl Supervised for TimeService {}
impl SystemService for TimeService {}

impl Default for TimeService {
    fn default() -> Self {
        TimeService {
            clients: Default::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ClientA;

impl Actor for ClientA {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("🐰 starting ClientA");
        TimeService::from_registry()
            .send(RegisterForTime(ctx.address().downgrade().recipient()))
            .into_actor(self)
            .then(|_, _slf, _| fut::ready(()))
            .spawn(ctx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        println!("🐰 stopping ClientA");
        Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("🐰 stopped ClientA");
    }
}

impl Handler<TimePing> for ClientA {
    type Result = ();

    fn handle(&mut self, msg: TimePing, _ctx: &mut Self::Context) -> Self::Result {
        println!("🐰 ClientA received ping: {:?}", msg);
    }
}

#[derive(Debug, Default)]
pub struct ClientB;

impl Actor for ClientB {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("🐇 starting ClientB");
        TimeService::from_registry()
            .send(RegisterForTime(ctx.address().downgrade().recipient()))
            .into_actor(self)
            .then(|_, _slf, _| fut::ready(()))
            .spawn(ctx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        println!("🐇  stopping ClientB");
        Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("🐇 stopped ClientB");
    }
}

impl Handler<TimePing> for ClientB {
    type Result = ();

    fn handle(&mut self, msg: TimePing, _ctx: &mut Self::Context) -> Self::Result {
        println!("🐇  ClientB received ping: {:?}", msg);
    }
}

#[actix::main]
async fn main() {
    {
        println!("🎩 creating client client A");
        let _client_a = ClientA.start();
        {
            println!("🎩 creating client client B");
            let _client_b = ClientB.start();

            println!("🎩 press Ctrl-C to stop client B");
            tokio::signal::ctrl_c().await.unwrap();
            println!("🎩 Ctrl-C received, stopping client");
        }

        println!("🎩 press Ctrl-C to stop client A");
        tokio::signal::ctrl_c().await.unwrap();
        println!("🎩 Ctrl-C received, stopping client");
    }

    tokio::signal::ctrl_c().await.unwrap();
    println!("🎩 Ctrl-C received, shutting down");
    System::current().stop();
}
