#![cfg_attr(feature="cargo-clippy", allow(let_unit_value))]
extern crate rand;
extern crate bytes;
extern crate byteorder;
extern crate tokio_io;
extern crate tokio_core;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate serde_derive;

extern crate actix;

use std::{io, net};
use std::str::FromStr;
use tokio_core::net::{TcpListener, TcpStream};
use actix::prelude::*;

mod codec;
mod server;
mod session;

use codec::ChatCodec;
use server::ChatServer;
use session::ChatSession;


/// Define tcp server that will accept incomint tcp connection and create
/// chat actors.
struct Server {
    chat: Address<ChatServer>,
}

/// Make actor from `Server`
impl Actor for Server {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;
}

/// Handle stream of TcpStream's
impl StreamHandler<(TcpStream, net::SocketAddr), io::Error> for Server {}

impl ResponseType<(TcpStream, net::SocketAddr)> for Server {
    type Item = ();
    type Error = ();
}

impl Handler<(TcpStream, net::SocketAddr), io::Error> for Server {

    fn handle(&mut self, msg: (TcpStream, net::SocketAddr), _: &mut Context<Self>)
              -> Response<Self, (TcpStream, net::SocketAddr)>
    {
        // For each incoming connection we create `ChatSession` actor
        // with out chat server address.
        let server = self.chat.clone();
        let _: () = ChatSession::new(server).framed(msg.0, ChatCodec);

        // this is response for message, which is defined by `ResponseType` trait
        // in this case we just return unit.
        Self::empty()
    }
}


fn main() {
    let sys = actix::System::new("chat-server");

    // Start chat server actor
    let server: Address<_> = ChatServer::default().start();

    // Create server listener
    let addr = net::SocketAddr::from_str("127.0.0.1:12345").unwrap();
    let listener = TcpListener::bind(&addr, Arbiter::handle()).unwrap();

    // Our chat server `Server` is an actor, first we need to start it
    // and then add stream on incoming tcp connections to it.
    // TcpListener::incoming() returns stream of the (TcpStream, net::SocketAddr) items
    // So to be able to handle this events `Server` actor has to implement
    // stream handler `StreamHandler<(TcpStream, net::SocketAddr), io::Error>`
    let _: () = Server::create(|ctx| {
        ctx.add_stream(listener.incoming());
        Server{chat: server}
    });

    println!("Running chat server on 127.0.0.1:12345");
    sys.run();
}
