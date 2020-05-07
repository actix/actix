use std::str::FromStr;
use std::time::Duration;
use std::{io, net, thread};

use futures_util::future::FutureExt;

use actix::prelude::*;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;
use tokio_util::codec::FramedRead;

mod codec;

#[actix_rt::main]
async fn main() {
    println!("Running chat client");

    // Connect to server
    let addr = net::SocketAddr::from_str("127.0.0.1:12345").unwrap();
    Arbiter::spawn(TcpStream::connect(addr).then(|stream| {
        let stream = stream.unwrap();
        let addr = ChatClient::create(|ctx| {
            let (r, w) = tokio::io::split(stream);
            ctx.add_stream(FramedRead::new(r, codec::ClientChatCodec));
            ChatClient {
                framed: actix::io::FramedWrite::new(w, codec::ClientChatCodec, ctx),
            }
        });

        // start console loop
        thread::spawn(move || loop {
            let mut cmd = String::new();
            if io::stdin().read_line(&mut cmd).is_err() {
                println!("error");
                return;
            }

            addr.do_send(ClientCommand(cmd));
        });

        async {}
    }));

    tokio::signal::ctrl_c().await.unwrap();
    println!("Ctrl-C received, shutting down");
    System::current().stop();
}

struct ChatClient {
    framed: actix::io::FramedWrite<
        codec::ChatRequest,
        WriteHalf<TcpStream>,
        codec::ClientChatCodec,
    >,
}

#[derive(Message)]
#[rtype(result = "()")]
struct ClientCommand(String);

impl Actor for ChatClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        // start heartbeats otherwise server will disconnect after 10 seconds
        self.hb(ctx)
    }

    fn stopping(&mut self, _: &mut Context<Self>) -> Running {
        println!("Disconnected");

        // Stop application on disconnect
        System::current().stop();

        Running::Stop
    }
}

impl ChatClient {
    fn hb(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(1, 0), |act, ctx| {
            act.framed.write(codec::ChatRequest::Ping);
            act.hb(ctx);
        });
    }
}

impl actix::io::WriteHandler<io::Error> for ChatClient {}

/// Handle stdin commands
impl Handler<ClientCommand> for ChatClient {
    type Result = ();

    fn handle(&mut self, msg: ClientCommand, _: &mut Context<Self>) {
        let m = msg.0.trim();

        // we check for /sss type of messages
        if m.starts_with('/') {
            let v: Vec<&str> = m.splitn(2, ' ').collect();
            match v.get(0) {
                Some(&"/list") => {
                    self.framed.write(codec::ChatRequest::List);
                }
                Some(&"/join") => {
                    if v.len() == 2 {
                        self.framed.write(codec::ChatRequest::Join(v[1].to_owned()));
                    } else {
                        println!("!!! room name is required");
                    }
                }
                _ => println!("!!! unknown command"),
            }
        } else {
            self.framed.write(codec::ChatRequest::Message(m.to_owned()));
        }
    }
}

/// Server communication
impl StreamHandler<Result<codec::ChatResponse, io::Error>> for ChatClient {
    fn handle(
        &mut self,
        msg: Result<codec::ChatResponse, io::Error>,
        _: &mut Context<Self>,
    ) {
        match msg {
            Ok(codec::ChatResponse::Message(ref msg)) => {
                println!("message: {}", msg);
            }
            Ok(codec::ChatResponse::Joined(ref msg)) => {
                println!("!!! joined: {}", msg);
            }
            Ok(codec::ChatResponse::Rooms(rooms)) => {
                println!("\n!!! Available rooms:");
                for room in rooms {
                    println!("{}", room);
                }
                println!();
            }
            _ => (),
        }
    }
}
