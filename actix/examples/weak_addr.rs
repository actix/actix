//! WeakAddr example
//!
//! With a weak address you can register an client actor's addrs with a
//! service and still get cleaned up correctly when the client actor is
//! stopped. Alternatively you'd have to check if the client's mailbox
//! is still open.

use actix::{prelude::*, WeakAddr};
use actix::{Actor, Context};

use std::time::{Duration, Instant};

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct TimePing(Instant);

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterForTime(pub WeakAddr<Client>);

#[derive(Debug, Default)]
pub struct TimeService {
    clients: Vec<WeakAddr<Client>>,
}

impl TimeService {
    fn send_tick(&mut self, _ctx: &mut Context<Self>) {
        for client in self.clients.iter() {
            if let Some(client) = client.upgrade() {
                client.do_send(TimePing(Instant::now()));
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

#[derive(Debug, Default)]
pub struct Client;

impl Actor for Client {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("🐰 starting Client");
        TimeService::from_registry()
            .send(RegisterForTime(ctx.address().downgrade()))
            .into_actor(self)
            .then(|_, _slf, _| fut::ready(()))
            .spawn(ctx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        println!("🐰 stopping Client");
        Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        println!("🐰 stopped Client");
    }
}

impl Handler<TimePing> for Client {
    type Result = ();

    fn handle(&mut self, msg: TimePing, _ctx: &mut Self::Context) -> Self::Result {
        println!("🐰 client received ping: {:?}", msg);
    }
}

#[actix::main]
async fn main() {
    {
        println!("🎩 creating client client");
        let _client = Client.start();

        println!("🎩 press Ctrl-C to stop client");
        tokio::signal::ctrl_c().await.unwrap();
        println!("🎩 Ctrl-C received, stopping client");
    }

    tokio::signal::ctrl_c().await.unwrap();
    println!("🎩 Ctrl-C received, shutting down");
    System::current().stop();
}
