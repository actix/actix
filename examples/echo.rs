extern crate actix;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio;

use actix::prelude::*;
use futures::{Future, Sink, Stream};
use futures::sync::mpsc::{self, UnboundedSender};
use std::io::{Error, ErrorKind};
use std::thread;
use tokio::codec::{Decoder, Encoder, FramedRead, FramedWrite, LinesCodec};
use tokio::io;

#[derive(Message)]
struct Message(pub String);

impl From<String> for Message {
    fn from(s: String) -> Self {
        Message(s)
    }
}

struct Stdout {
    tx: UnboundedSender<String>,
}

impl Stdout {
    pub fn new<E>(codec: E) -> Self
        where E: Encoder<Item=String, Error=Error> + Send + Clone + 'static
    {
        let (tx, rx) = mpsc::unbounded();
        thread::spawn(|| {
            tokio::run(rx.for_each(move |msg| {
                FramedWrite::new(io::stdout(), codec.clone())
                    .send(msg)
                    .map(|_| ())
                    .map_err(|err| error!("STDOUT Error = {}", err))
            }));
        });
        Stdout {
            tx: tx
        }
    }
}

impl Actor for Stdout {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        trace!("STDOUT started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("STDOUT stopped");
    }
}

impl Handler<Message> for Stdout {
    type Result = ();

    fn handle(&mut self, msg: Message, _ctx: &mut Context<Self>) {
        self.tx.clone().send(msg.0).wait().expect("Send message");
    }
}

impl Default for Stdout {
    fn default() -> Self {
        Stdout::new(LinesCodec::new())
    }
}

struct Stdin;

impl Stdin {
    pub fn new<D, M, R>(codec: D, recipient: Recipient<M>) -> Self
        where D: Decoder<Item=String, Error=Error> + Send + 'static,
              M: actix::Message<Result=R> + std::convert::From<String> + Send + 'static,
              R: Send + 'static
    {
        thread::spawn(|| {
            tokio::run(FramedRead::new(io::stdin(), codec)
                .for_each(move |msg| {
                    recipient.do_send(msg.into())
                        .map_err(|_| Error::new(ErrorKind::Other, "Send Error"))
                })
                .map_err(|err| {
                    error!("STDIN Error = {}", err);
                })
            );
        });
        Stdin {}
    }
}

impl Actor for Stdin {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        trace!("STDIN started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("STDIN stopped");
    }
}

fn main() {
    let code = System::run(|| {
        Stdin::new(
            LinesCodec::new(),
            Stdout::default().start().recipient()
        ).start();
    });
    std::process::exit(code);
}
