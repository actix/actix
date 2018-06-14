#[macro_use]
extern crate actix;
extern crate byteorder;
extern crate bytes;
extern crate futures;
extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_tcp;
#[macro_use]
extern crate serde_derive;

use actix::prelude::*;
use futures::Future;
use std::str::FromStr;
use std::time::Duration;
use std::{io, net, process, thread};
use tokio_io::codec::FramedRead;
use tokio_io::io::WriteHalf;
use tokio_io::AsyncRead;
use tokio_tcp::TcpStream;

mod codec;

fn main() {
    println!("Running chat client");

    actix::System::run(|| {
        // Connect to server
        let addr = net::SocketAddr::from_str("127.0.0.1:12345").unwrap();
        tokio::spawn(
            TcpStream::connect(&addr)
                .and_then(|stream| {
                    let addr = ChatClient::create(|ctx| {
                        let (r, w) = stream.split();
                        ctx.add_stream(FramedRead::new(r, codec::ClientChatCodec));
                        ChatClient {
                            framed: actix::io::FramedWrite::new(
                                w,
                                codec::ClientChatCodec,
                                ctx,
                            ),
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

                    futures::future::ok(())
                })
                .map_err(|e| {
                    println!("Can not connect to server: {}", e);
                    process::exit(1)
                }),
        );
    });
}

struct ChatClient {
    framed: actix::io::FramedWrite<WriteHalf<TcpStream>, codec::ClientChatCodec>,
}

#[derive(Message)]
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
            match v[0] {
                "/list" => {
                    self.framed.write(codec::ChatRequest::List);
                }
                "/join" => {
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
impl StreamHandler<codec::ChatResponse, io::Error> for ChatClient {
    fn handle(
        &mut self, msg: io::Result<Option<codec::ChatResponse>>, ctx: &mut Context<Self>,
    ) {
        match msg {
            Ok(Some(codec::ChatResponse::Message(ref msg))) => {
                println!("message: {}", msg);
            }
            Ok(Some(codec::ChatResponse::Joined(ref msg))) => {
                println!("!!! joined: {}", msg);
            }
            Ok(Some(codec::ChatResponse::Rooms(rooms))) => {
                println!("\n!!! Available rooms:");
                for room in rooms {
                    println!("{}", room);
                }
                println!();
            }
            _ => ctx.stop(),
        }
    }
}
