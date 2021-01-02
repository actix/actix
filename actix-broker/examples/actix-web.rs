use actix::prelude::*;
use actix_broker::{Broker, BrokerSubscribe, SystemBroker};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer};
use std::io;

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
struct Hello;

struct TestActor;

impl Actor for TestActor {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_async::<SystemBroker, Hello>(ctx);
    }
}

impl Handler<Hello> for TestActor {
    type Result = ();
    fn handle(&mut self, msg: Hello, _ctx: &mut Self::Context) {
        println!("TestActor: Received {:?}", msg);
    }
}

async fn index(_req: HttpRequest) -> Result<HttpResponse, Error> {
    Broker::<SystemBroker>::issue_async(Hello);
    Ok(HttpResponse::Ok()
        .content_type("text/plain")
        .body("Welcome!"))
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    TestActor.start();

    HttpServer::new(|| {
        App::new().service(
            web::scope("/").service(web::resource("").route(web::get().to(index))),
        )
    })
    .bind("127.0.0.1:8080")
    .unwrap()
    .run()
    .await
}
