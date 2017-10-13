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
    type Context = FramedContext<Self>;
}

/// To use `FramedContext` we have to define Io type and Codec
impl FramedActor for ChatSession {
    type Io = TcpStream;
    type Codec= ChatCodec;
}

/// Also `FramedContext` requires Actor which is able to handle stream
/// of `<Codec as Decoder>::Item` items.
impl StreamHandler<ChatRequest, io::Error> for ChatSession {

    fn started(&mut self, ctx: &mut FramedContext<Self>) {
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
            fut::ok(())
        }).wait(ctx);
    }

    fn finished(&mut self, ctx: &mut FramedContext<Self>) {
        // notify chat server
        self.addr.send(server::Disconnect{id: self.id});

        ctx.stop()
    }
}

impl ResponseType<ChatRequest> for ChatSession {
    type Item = ();
    type Error = ();
}

impl Handler<ChatRequest, io::Error> for ChatSession {

    /// We'll stop chat session actor on any error, high likely it is just
    /// termination of the tcp stream.
    fn error(&mut self, _: io::Error, ctx: &mut FramedContext<Self>) {
        ctx.stop()
    }

    /// This is main event loop for client requests
    fn handle(&mut self, msg: ChatRequest, ctx: &mut FramedContext<Self>)
              -> Response<Self, ChatRequest>
    {
        match msg {
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
                    fut::ok(())
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

        Self::empty()
    }
}

/// Handler for Message, chat server sends this message, we just send string to peer
impl Handler<Message> for ChatSession {

    fn handle(&mut self, msg: Message, ctx: &mut FramedContext<Self>)
              -> Response<Self, Message>
    {
        // send message to peer
        let _ = ctx.send(ChatResponse::Message(msg.0));

        Self::empty()
    }
}

impl ResponseType<Message> for ChatSession {
    type Item = ();
    type Error = ();
}


/// Helper methods
impl ChatSession {

    pub fn new(addr: Address<ChatServer>) -> ChatSession {
        ChatSession {id: 0, addr: addr, hb: Instant::now(), room: "Main".to_owned()}
    }
    
    /// helper method that sends ping to client every second.
    ///
    /// also this method check heartbeats from client
    fn hb(&self, ctx: &mut FramedContext<Self>) {
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
