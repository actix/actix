#![allow(clippy::let_unit_value)]
use std::net;
use std::str::FromStr;

use actix::prelude::*;
use futures::Stream;
use tokio_codec::FramedRead;
use tokio_io::AsyncRead;
use tokio_tcp::{TcpListener, TcpStream};

mod codec;
mod server;
mod session;

use codec::ChatCodec;
use server::ChatServer;
use session::ChatSession;

/// Define tcp server that will accept incoming tcp connection and create
/// chat actors.
struct Server {
    chat: Addr<ChatServer>,
}

/// Make actor from `Server`
impl Actor for Server {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;
}

#[derive(Message)]
struct TcpConnect(pub TcpStream, pub net::SocketAddr);

/// Handle stream of TcpStream's
impl Handler<TcpConnect> for Server {
    /// this is response for message, which is defined by `ResponseType` trait
    /// in this case we just return unit.
    type Result = ();

    fn handle(&mut self, msg: TcpConnect, _: &mut Context<Self>) {
        // For each incoming connection we create `ChatSession` actor
        // with out chat server address.
        let server = self.chat.clone();
        ChatSession::create(move |ctx| {
            let (r, w) = msg.0.split();
            ChatSession::add_stream(FramedRead::new(r, ChatCodec), ctx);
            ChatSession::new(server, actix::io::FramedWrite::new(w, ChatCodec, ctx))
        });
    }
}

fn main() -> std::io::Result<()> {
    actix::System::run(|| {
        // Start chat server actor
        let server = ChatServer::default().start();

        // Create server listener
        let addr = net::SocketAddr::from_str("127.0.0.1:12345").unwrap();
        let listener = TcpListener::bind(&addr).unwrap();

        // Our chat server `Server` is an actor, first we need to start it
        // and then add stream on incoming tcp connections to it.
        // TcpListener::incoming() returns stream of the (TcpStream, net::SocketAddr)
        // items So to be able to handle this events `Server` actor has to implement
        // stream handler `StreamHandler<(TcpStream, net::SocketAddr), io::Error>`
        Server::create(|ctx| {
            ctx.add_message_stream(listener.incoming().map_err(|_| ()).map(|st| {
                let addr = st.peer_addr().unwrap();
                TcpConnect(st, addr)
            }));
            Server { chat: server }
        });

        let ctrl_c = tokio_signal::ctrl_c().flatten_stream();
        let handle_shutdown = ctrl_c
            .for_each(|()| {
                println!("Ctrl-C received, shutting down");
                System::current().stop();
                Ok(())
            })
            .map_err(|_| ());
        actix::spawn(handle_shutdown);

        println!("Running chat server on 127.0.0.1:12345");
    })
}
