//! `ClientSession` is an actor, it manages peer tcp connection and
//! proxies commands from peer to `ChatServer`.
use actix::prelude::*;
use std::io;
use std::time::{Duration, Instant};
use tokio_io::io::WriteHalf;
use tokio_tcp::TcpStream;

use codec::{ChatCodec, ChatRequest, ChatResponse};
use server::{self, ChatServer};

/// Chat server sends this messages to session
#[derive(Message)]
pub struct Message(pub String);

/// `ChatSession` actor is responsible for tcp peer communications.
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
    framed: actix::io::FramedWrite<WriteHalf<TcpStream>, ChatCodec>,
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
            .then(|res, act, ctx| {
                match res {
                    Ok(res) => act.id = res,
                    // something is wrong with chat server
                    _ => ctx.stop(),
                }
                actix::fut::ok(())
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
impl StreamHandler2<ChatRequest, io::Error> for ChatSession {
    /// This is main event loop for client requests
    fn handle(&mut self, msg: io::Result<Option<ChatRequest>>, ctx: &mut Self::Context) {
        match msg {
            Ok(Some(ChatRequest::List)) => {
                // Send ListRooms message to chat server and wait for response
                println!("List rooms");
                self.addr.send(server::ListRooms)
                    .into_actor(self)     // <- create actor compatible future
                    .then(|res, act, _| {
                        match res {
                            Ok(rooms) => act.framed.write(ChatResponse::Rooms(rooms)),
                            _ => println!("Something is wrong"),
                        }
                        actix::fut::ok(())
                    }).wait(ctx)
                // .wait(ctx) pauses all events in context,
                // so actor wont receive any new messages until it get list of rooms back
            }
            Ok(Some(ChatRequest::Join(name))) => {
                println!("Join to room: {}", name);
                self.room = name.clone();
                self.addr.do_send(server::Join {
                    id: self.id,
                    name: name.clone(),
                });
                self.framed.write(ChatResponse::Joined(name));
            }
            Ok(Some(ChatRequest::Message(message))) => {
                // send message to chat server
                println!("Peer message: {}", message);
                self.addr.do_send(server::Message {
                    id: self.id,
                    msg: message,
                    room: self.room.clone(),
                })
            }
            // we update heartbeat time on ping from peer
            Ok(Some(ChatRequest::Ping)) => self.hb = Instant::now(),
            // stop on error
            _ => ctx.stop(),
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
        framed: actix::io::FramedWrite<WriteHalf<TcpStream>, ChatCodec>,
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
