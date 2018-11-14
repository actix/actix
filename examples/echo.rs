//! This example echos, forwards, or copies, lines from STDIN to STDOUT until
//! either the End-of-File (EOF) marker is reached on STDIN or the application
//! is terminated.
//!
//! Two actors are created: (1) [`Stdin`] and (2) [`Stdout`]. The Stdin actor
//! reads lines from STDIN and sends the lines to a recipient. In this case, the
//! recipient is the Stdout actor. The Stdin actor only receives [`Input`]
//! messages, which could come from another actor, but typically only come from
//! the STDIN thread. The Stdout actor receives [`Output`] messages from any
//! actor. In this case, the Stdout actor receives messages from the Stdin actor.
//! The Stdout actor sends the Output messages to the STDOUT thread via a
//! channel.
//!
//! # Background
//!
//! Reading and writing to STDIN and STDOUT within the [tokio runtime] is not
//! straight-forward because these are blocking actions. The current
//! recommendation is to have reading from STDIN and writing to STDOUT in
//! separate threads from the tokio runtime thread and use channels to pass data
//! between the three threads. This is discussed in tokio's [Issue 374] and
//! tokio-process's [Issue 7]. The [tokio-stdin] and [tokio-stdout] crates are
//! implementations based on these recommendations for reading and writing from
//! STDIN and STDOUT within the tokio runtime, respectively. Thus, the [`Stdin`]
//! and [`Stdout`] actors are implemented in a similar fashion.
//!
//! [`Input`]: struct.Input.html
//! [Issue 374]: https://github.com/tokio-rs/tokio/issues/374
//! [Issue 7]: https://github.com/alexcrichton/tokio-process/issues/7
//! [`Stdin`]: struct.Stdin.html
//! [`Stdout`]: struct.Stdout.html
//! [tokio-stdin]: https://crates.io/crates/tokio-stdin
//! [tokio-stdout]: https://crates.io/crates/tokio-stdout
//! [tokio runtime]: https://tokio.rs/blog/2018-03-tokio-runtime/

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

/// The message received by the [`Stdout`] actor.
///
/// The Stdout actor writes the message, which is just a thin wrapper around a
/// series of bytes, to STDOUT.
///
/// [`Stdout`]: struct.Stdout.html
#[derive(Message)]
struct Output(pub Bytes);

// A simple conversion of an [`Input`] message to an [`Output`] message so that
// the Stdin actor can easily "forward" its messages to the Stdout actor.
//
// [`Input`]: struct.Input.html
// [`Output`]: struct.Output.html
impl From<Input> for Output {
    fn from(i: Input) -> Self {
        Output(i.0.freeze())
    }
}

/// An actor that writes bytes to STDOUT.
///
/// This actor starts a separate thread to avoid blocking and uses a channel to
/// send bytes it has received as an [`Output`] message to the separate thread.
/// The channel only has one sender (tx). When this actor is dropped, the sender
/// is dropped. This causes the receiver, which is implemented as a
/// [`Stream`]/[`Future`] to become resolved, i.e. completed. The resolution of
/// the receiver stops the loop within the STDOUT thread and causes the actix
/// system to shutdown.
///
/// [`Future`]: https://docs.rs/futures/0.1.25/futures/future/trait.Future.html
/// [`Output`]: struct.Output.html
/// [`Stream`]: https://docs.rs/futures/0.1.25/futures/stream/trait.Stream.html
struct Stdout {
    tx: UnboundedSender<Bytes>,
}

impl Stdout {
    /// Creates a new `Stdout` actor from an actix system and encoder.
    ///
    /// This will spawn a separate thread to avoid blocking within the
    /// asynchronous, single-threaded execution of the actix system/tokio
    /// runtime. The separate thread will stop when this actor is dropped _and_
    /// when the separate thread stops, it will shutdown the actix system as
    /// well. In this way, a "clean" shutdown occurs.
    ///
    /// Any encoder that encodes a message as a series of bytes can be used.
    /// This means any codec provided by the tokio crate under the
    /// [`tokio::codec`] module can be used, except for the
    /// [`tokio::codec::LinesCodec`] because it encodes messages as strings and
    /// not bytes. However, the `tokio::codec::LinesCodec` can be used by
    /// wrapping it in a new type that converts strings into bytes.
    ///
    /// [`tokio::codec`]: https://tokio-rs.github.io/tokio/tokio/codec/index.html
    /// [`tokio::codec::LinesCodec`]: https://tokio-rs.github.io/tokio/tokio/codec/struct.LinesCodec.html
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

// Added trace statements to help demonstrate the interaction between the Stdin
// and Stdout actors.
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

// The Stdout actor receives Output messages.
//
// The handler of the Output messages for the Stdout actor is relatively simple.
// The Output message is sent to the STDOUT thread using the internal channel.
impl Handler<Output> for Stdout {
    type Result = ();

    fn handle(&mut self, msg: Output, _ctx: &mut Context<Self>) {
        trace!("STDOUT handle");
        self.tx.clone().send(msg.0).wait().expect("Send message");
    }
}

// A little conversion helper to clean up creation of a Stdout actor from an
// encoder/codec. Uses the current actix system.
impl<E> From<E> for Stdout
    where E: Encoder<Item=Bytes, Error=Error> + Send + Clone + 'static
{
    fn from(e: E) -> Self {
        Stdout::new(System::current(), e)
    }
}

/// An actor that reads bytes from STDIN.
///
/// This actor starts a separate thread to avoid blocking and uses a channel to
/// receive bytes at an [`Input`] message from the separate thread. The channel
/// only has one sender (tx). When STDIN receives an End-of-File (EOF) signal,
/// the sender will be dropped. This causes the receiver within this actor to
/// become resolved, i.e. completed. The resolution of the receiver stops this
/// actor.
///
/// [`Input`]: struct.Input.html
struct Stdin<D> {
    recipient: Recipient<Input>,
    decoder: D,
}

impl<D> Stdin<D>
    where D: Decoder + Send + Clone + 'static
{
    /// Creates a new `Stdin` actor from a decoder and recipient.
    ///
    /// The separate thread will be spawn when this actor is started, unlike the
    /// [`Stdout`] actor, which starts a separate thread when the [`new`] method
    /// is executed. The separate thread will only stop when it reaches the
    /// End-of-File (EOF) for STDIN.
    ///
    /// Any decoder that decodes data from STDIN into a series of bytes can be
    /// used. This mean any codec provided by the tokio crate under the
    /// [`tokio::codec`] module can be used, except for the
    /// [`tokio::codec::LinesCodec`] because it decodes data into strings and
    /// not bytes. However, the `tokio::codec::LinesCodec` can be used by
    /// wrapping it in a new type that converts strings into bytes.
    ///
    /// [`Stdout`]: struct.Stdout.html
    /// [`tokio::codec`]: https://tokio-rs.github.io/tokio/tokio/codec/index.html
    /// [`tokio::codec::LinesCodec`]: https://tokio-rs.github.io/tokio/tokio/codec/struct.LinesCodec.html
    pub fn new(decoder: D, recipient: Recipient<Input>) -> Self {
        Stdin {
            recipient: recipient,
            decoder: decoder,
        }
    }
}

// A separate thread is started when this actor is started. The separate thread
// reads bytes from STDIN and sends them to this actor using an internal
// channel. This implementation of the Actor trait also includes trace
// logging statements to demonstrate the interaction with the Stdout actor.
impl<D> Actor for Stdin<D>
    where D: Decoder<Item=BytesMut, Error=Error> + Send + Clone + 'static
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        trace!("STDIN started");
        let (tx, rx) = mpsc::unbounded();
        let codec = self.decoder.clone();
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
        ctx.add_stream(rx);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        trace!("STDIN stopping");
        Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("STDIN stopped");
    }
}

impl<D> StreamHandler<BytesMut, ()> for Stdin<D>
    where D: Decoder<Item=BytesMut, Error=Error> + Send + Clone + 'static
{
    fn handle(&mut self, item: BytesMut, _ctx: &mut Self::Context) {
        trace!("STDIN stream handle");
        self.recipient.do_send(item.into()).unwrap();
    }
}

struct Echo {
    stdout: Addr<Stdout>,
}

impl Echo {
    pub fn new(stdout: Addr<Stdout>) -> Self {
        Echo {
            stdout: stdout,
        }
    }
}

impl Actor for Echo {
    type Context = Context<Self>;
}

impl Handler<Input> for Echo {
    type Result = ();

    fn handle(&mut self, item: Input, _ctx: &mut Self::Context) {
        trace!("ECHO handle");
        self.stdout.do_send(item.into())
    }
}

impl From<Addr<Stdout>> for Echo {
    fn from(s: Addr<Stdout>) -> Self {
        Echo::new(s)
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
            Echo::new(Stdout::from(LinesCodec::default()).start()).start().recipient()
        ).start();
    });
    std::process::exit(code);
}
