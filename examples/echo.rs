extern crate actix;
extern crate env_logger;
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
    pub fn new<E>(sys: System, codec: E) -> Self
        where E: Encoder<Item=String, Error=Error> + Send + Clone + 'static
    {
        let (tx, rx) = mpsc::unbounded();
        thread::spawn(move || {
            info!("Begin STDOUT thread");
            tokio::run(rx.for_each(move |msg| {
                FramedWrite::new(io::stdout(), codec.clone())
                    .send(msg)
                    .map(|_| ())
                    .map_err(|err| error!("STDOUT Error = {}", err))
            }));
            info!("End STDOUT thread");
            sys.stop();
        });
        Stdout {
            tx: tx,
        }
    }
}

impl Actor for Stdout {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        trace!("STDOUT started");
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        trace!("STDOUT stopping");
        Running::Stop
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
        Stdout::new(System::current(), LinesCodec::new())
    }
}

impl<E> From<E> for Stdout
    where E: Encoder<Item=String, Error=Error> + Send + Clone + 'static
{
    fn from(e: E) -> Self {
        Stdout::new(System::current(), e)
    }
}

struct Stdin<D> {
    recipient: Recipient<Message>,
    codec: D,
}

impl<D> Stdin<D>
    where D: Decoder<Item=String, Error=Error> + Send + Clone + 'static
{
    pub fn new(codec: D, recipient: Recipient<Message>) -> Self {
        Stdin {
            recipient: recipient,
            codec: codec,
        }
    }
}

impl<D> Actor for Stdin<D>
    where D: Decoder<Item=String, Error=Error> + Send + Clone + 'static
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        trace!("STDIN started");
        let (tx, rx) = mpsc::unbounded();
        let codec = self.codec.clone();
        thread::spawn(|| {
            info!("Begin STDIN thread");
            tokio::run(FramedRead::new(io::stdin(), codec)
                .for_each(move |msg| {
                    tx.clone()
                        .send(msg.into())
                        .map(|_| ())
                        .map_err(|_| Error::new(ErrorKind::Other, "Send Error"))
                })
                .map_err(|err| {
                    error!("STDIN Error = {}", err);
                })
            );
            info!("End STDIN thread");
        });
        ctx.add_message_stream(rx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        trace!("STDIN stopping");
        Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("STDIN stopped");
    }
}

impl<D> Handler<Message> for Stdin<D>
    where D: Decoder<Item=String, Error=Error> + Send + Clone + 'static
{
    type Result = ();

    fn handle(&mut self, item: Message, _ctx: &mut Self::Context) {
        self.recipient.do_send(item).unwrap();
    }
}

fn main() {
    env_logger::init();
    let code = System::run(|| {
        Stdin::new(
            LinesCodec::new(),
            Stdout::default().start().recipient()
        ).start();
    });
    std::process::exit(code);
}
