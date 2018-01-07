//! `ClientSession` is an actor, it manages peer tcp connection and
//! proxies commands from peer to `ChatServer`.
use std::io;
use std::time::{Instant, Duration};
use tokio_core::net::TcpStream;
use actix::prelude::*;

use server::{self, ChatServer};
use codec::{ChatRequest, ChatResponse, ChatCodec};


/// Chat server sends this messages to session
pub struct Message(pub String);

impl ResponseType for Message {
    type Item = ();
    type Error = ();
}

/// `ChatSession` actor is responsible for tcp peer communitions.
pub struct ChatSession {
    /// unique session id
    id: usize,
    /// this is address of chat server
    addr: Address<ChatServer>,
    /// Client must send ping at least once per 10 seconds, otherwise we drop connection.
    hb: Instant,
    /// joined room
    room: String,
}

impl Actor for ChatSession {
    /// For tcp communication we are going to use `FramedContext`.
    /// It is convinient wrapper around `Framed` object from `tokio_io`
    type Context = actix::FramedContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // we'll start heartbeat process on session start.
        self.hb(ctx);

        // register self in chat server. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        self.addr.call(self, server::Connect{addr: ctx.address()}).then(|res, act, ctx| {
            match res {
                Ok(Ok(res)) => act.id = res,
                // something is wrong with chat server
                _ => ctx.stop(),
            }
            actix::fut::ok(())
        }).wait(ctx);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> bool {
        // notify chat server
        self.addr.send(server::Disconnect{id: self.id});
        true
    }
}

/// To use `FramedContext` we have to define Io type and Codec
impl actix::FramedActor for ChatSession {
    type Io = TcpStream;
    type Codec= ChatCodec;

    /// We'll stop chat session actor on any error, high likely it is just
    /// termination of the tcp stream.
    fn error(&mut self, _: io::Error, ctx: &mut Self::Context) {
        ctx.stop()
    }

    /// This is main event loop for client requests
    fn handle(&mut self, msg: io::Result<ChatRequest>, ctx: &mut Self::Context) {
        match msg {
            Err(_) => ctx.stop(),
            Ok(msg) => match msg {
                ChatRequest::List => {
                    // Send ListRooms message to chat server and wait for response
                    println!("List rooms");
                    self.addr.call(self, server::ListRooms).then(|res, _, ctx| {
                        match res {
                            Ok(Ok(rooms)) => {
                                let _ = ctx.send(ChatResponse::Rooms(rooms));
                            },
                            _ => println!("Something is wrong"),
                        }
                        actix::fut::ok(())
                    }).wait(ctx)
                    // .wait(ctx) pauses all events in context,
                    // so actor wont receive any new messages until it get list of rooms back
                },
                ChatRequest::Join(name) => {
                    println!("Join to room: {}", name);
                    self.room = name.clone();
                    self.addr.send(server::Join{id: self.id, name: name.clone()});
                    let _ = ctx.send(ChatResponse::Joined(name));
                },
                ChatRequest::Message(message) => {
                    // send message to chat server
                    println!("Peer message: {}", message);
                    self.addr.send(
                        server::Message{id: self.id,
                                        msg: message, room:
                                        self.room.clone()})
                }
                // we update heartbeat time on ping from peer
                ChatRequest::Ping =>
                    self.hb = Instant::now(),
            }
        }
    }
}

/// Handler for Message, chat server sends this message, we just send string to peer
impl Handler<Message> for ChatSession {
    type Result = ();

    fn handle(&mut self, msg: Message, ctx: &mut Self::Context) {
        // send message to peer
        let _ = ctx.send(ChatResponse::Message(msg.0));
    }
}

/// Helper methods
impl ChatSession {

    pub fn new(addr: Address<ChatServer>) -> ChatSession {
        ChatSession {id: 0, addr: addr, hb: Instant::now(), room: "Main".to_owned()}
    }
    
    /// helper method that sends ping to client every second.
    ///
    /// also this method check heartbeats from client
    fn hb(&self, ctx: &mut actix::FramedContext<Self>) {
        ctx.run_later(Duration::new(1, 0), |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > Duration::new(10, 0) {
                // heartbeat timed out
                println!("Client heartbeat failed, disconnecting!");

                // notify chat server
                act.addr.send(server::Disconnect{id: act.id});

                // stop actor
                ctx.stop();
            }

            if ctx.send(ChatResponse::Ping).is_ok() {
                // if we can not send message to sink, sink is closed (disconnected)
                act.hb(ctx);
            }
        });
    }
}
