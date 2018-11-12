//! This example copies, or forwards, lines from STDIN to STDOUT until either
//! the End-of-File (EOF) marker is reached on STDIN or the application is
//! terminated.
//!
//! Reading and writing to STDIN and STDOUT within the [tokio runtime] is not
//! straight-forward because these are blocking actions. The current
//! recommendation is to have reading from STDIN and writing to STDOUT in
//! separate threads from the tokio runtime thread and use channels to pass data
//! between the three threads. This is discussed in tokio's [Issue 374] and
//! tokio-process [Issue 7]. The [tokio-stdin] and [tokio-stdout] crates are
//! implementations based on these recommendations for reading and writing from
//! STDIN and STDOUT with the tokio runtime, respectively. Thus, the [`Stdin`]
//! and [`Stdout`] actors are implemented in a similar fashion.
//!
//! [tokio runtime]: https://tokio.rs/blog/2018-03-tokio-runtime/
//! [Issue 374]: https://github.com/tokio-rs/tokio/issues/374
//! [Issue 7]: https://github.com/alexcrichton/tokio-process/issues/7
//! [tokio-stdin]: https://crates.io/crates/tokio-stdin
//! [tokio-stdout]: https://crates.io/crates/tokio-stdout
//! [`Stdin`]: #struct.stdin
//! [`Stdout`]: #struct.stdout

extern crate actix;
extern crate bytes;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate futures;
extern crate tokio;

use actix::prelude::*;
use bytes::{Bytes, BytesMut};
use futures::{Future, Sink, Stream};
use futures::sync::mpsc::{self, UnboundedSender};
use std::io::{Error, ErrorKind};
use std::thread;
use tokio::codec::{Decoder, Encoder, FramedRead, FramedWrite};
use tokio::io;

#[derive(Message)]
struct Input(pub BytesMut);

impl From<BytesMut> for Input {
    fn from(b: BytesMut) -> Self {
        Input(b)
    }
}

#[derive(Message)]
struct Output(pub Bytes);

impl From<Input> for Output {
    fn from(i: Input) -> Self {
        Output(i.0.freeze())
    }
}

struct Stdout {
    tx: UnboundedSender<Bytes>,
}

impl Stdout {
    pub fn new<E>(sys: System, encoder: E) -> Self
        where E: Encoder<Item=Bytes, Error=Error> + Send + Clone + 'static
    {
        let (tx, rx) = mpsc::unbounded();
        thread::spawn(move || {
            info!("Begin STDOUT thread");
            tokio::run(rx.for_each(move |msg| {
                FramedWrite::new(io::stdout(), encoder.clone())
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

impl Handler<Output> for Stdout {
    type Result = ();

    fn handle(&mut self, msg: Output, _ctx: &mut Context<Self>) {
        trace!("STDOUT handle");
        self.tx.clone().send(msg.0).wait().expect("Send message");
    }
}

impl<E> From<E> for Stdout
    where E: Encoder<Item=Bytes, Error=Error> + Send + Clone + 'static
{
    fn from(e: E) -> Self {
        Stdout::new(System::current(), e)
    }
}

struct Stdin<D> {
    recipient: Recipient<Output>,
    codec: D,
}

impl<D> Stdin<D>
    where D: Decoder + Send + Clone + 'static
{
    pub fn new(codec: D, recipient: Recipient<Output>) -> Self {
        Stdin {
            recipient: recipient,
            codec: codec,
        }
    }
}

impl<D> Actor for Stdin<D>
    where D: Decoder<Item=BytesMut, Error=Error> + Send + Clone + 'static
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

impl<D> Handler<Input> for Stdin<D>
    where D: Decoder<Item=BytesMut, Error=Error> + Send + Clone + 'static
{
    type Result = ();

    fn handle(&mut self, item: Input, _ctx: &mut Self::Context) {
        trace!("STDIN handle");
        self.recipient.do_send(item.into()).unwrap();
    }
}

#[derive(Clone)]
struct LinesCodec {
    inner: tokio::codec::LinesCodec,
}

impl Encoder for LinesCodec {
    type Item = Bytes;
    type Error = Error;

    fn encode(&mut self, data: Self::Item, buf: &mut BytesMut) -> Result<(), Self::Error> {
        self.inner.encode(String::from_utf8(data.to_vec()).expect("Valid UTF-8"), buf)
    }
}

impl Decoder for LinesCodec {
    type Item = BytesMut;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.inner.decode(src).map(|i| i.map(BytesMut::from))
    }
}

impl Default for LinesCodec {
    fn default() -> Self {
        LinesCodec {
            inner: tokio::codec::LinesCodec::new(),
        }
    }
}

fn main() {
    env_logger::init();
    let code = System::run(|| {
        Stdin::new(
            LinesCodec::default(),
            Stdout::from(LinesCodec::default()).start().recipient()
        ).start();
    });
    std::process::exit(code);
}
