//! `ClientSession` is an actor, it manages peer TCP connection and
//! proxies commands from peer to `ChatServer`.
use std::io;
use std::time::Duration;

use actix::clock::Instant;
use actix::prelude::*;

use tokio::io::WriteHalf;
use tokio::net::TcpStream;

use crate::codec::{ChatCodec, ChatRequest, ChatResponse};
use crate::server::{self, ChatServer};

/// Chat server sends this messages to session
#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub String);

/// `ChatSession` actor is responsible for TCP peer communications.
pub struct ChatSession {
    /// unique session id
    id: usize,
    /// this is address of chat server
    addr: Addr<ChatServer>,
    /// Client must send ping at least once per 10 seconds, otherwise we drop
    /// connection.
    hb: Instant,
    /// joined room
    room: String,
    /// Framed wrapper
    framed: actix::io::FramedWrite<ChatResponse, WriteHalf<TcpStream>, ChatCodec>,
}

impl Actor for ChatSession {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // we'll start heartbeat process on session start.
        self.hb(ctx);

        // register self in chat server. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        self.addr
            .send(server::Connect {
                addr: ctx.address(),
            })
            .into_actor(self)
            .then(move |res, act, _| {
                act.id = res.unwrap();
                async {}.into_actor(act)
            })
            .wait(ctx);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // notify chat server
        self.addr.do_send(server::Disconnect { id: self.id });
        Running::Stop
    }
}

impl actix::io::WriteHandler<io::Error> for ChatSession {}

/// To use `Framed` with an actor, we have to implement `StreamHandler` trait
impl StreamHandler<Result<ChatRequest, io::Error>> for ChatSession {
    /// This is main event loop for client requests
    fn handle(&mut self, msg: Result<ChatRequest, io::Error>, ctx: &mut Self::Context) {
        match msg {
            Ok(ChatRequest::List) => {
                // Send ListRooms message to chat server and wait for response
                println!("List rooms");
                self.addr
                    .send(server::ListRooms)
                    .into_actor(self) // <- create actor compatible future
                    .then(|res, act, _| {
                        match res {
                            Ok(rooms) => act.framed.write(ChatResponse::Rooms(rooms)),
                            _ => println!("Something is wrong"),
                        };
                        async {}.into_actor(act)
                    })
                    .wait(ctx)
                // .wait(ctx) pauses all events in context,
                // so actor wont receive any new messages until it get list of rooms back
            }
            Ok(ChatRequest::Join(name)) => {
                println!("Join to room: {}", name);
                self.room = name.clone();
                self.addr.do_send(server::Join {
                    id: self.id,
                    name: name.clone(),
                });
                self.framed.write(ChatResponse::Joined(name));
            }
            Ok(ChatRequest::Message(message)) => {
                // send message to chat server
                println!("Peer message: {}", message);
                self.addr.do_send(server::Message {
                    id: self.id,
                    msg: message,
                    room: self.room.clone(),
                })
            }
            // we update heartbeat time on ping from peer
            Ok(ChatRequest::Ping) => self.hb = Instant::now(),
            _ => unimplemented!(),
        }
    }
}

/// Handler for Message, chat server sends this message, we just send string to
/// peer
impl Handler<Message> for ChatSession {
    type Result = ();

    fn handle(&mut self, msg: Message, _: &mut Self::Context) {
        // send message to peer
        self.framed.write(ChatResponse::Message(msg.0));
    }
}

/// Helper methods
impl ChatSession {
    pub fn new(
        addr: Addr<ChatServer>,
        framed: actix::io::FramedWrite<ChatResponse, WriteHalf<TcpStream>, ChatCodec>,
    ) -> ChatSession {
        ChatSession {
            addr,
            framed,
            id: 0,
            hb: Instant::now(),
            room: "Main".to_owned(),
        }
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method check heartbeats from client
    fn hb(&self, ctx: &mut actix::Context<Self>) {
        ctx.run_later(Duration::new(1, 0), |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > Duration::new(10, 0) {
                // heartbeat timed out
                println!("Client heartbeat failed, disconnecting!");

                // notify chat server
                act.addr.do_send(server::Disconnect { id: act.id });

                // stop actor
                ctx.stop();
            }

            act.framed.write(ChatResponse::Ping);
            act.hb(ctx);
        });
    }
}
