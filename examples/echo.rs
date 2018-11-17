//! This example echos, forwards, or copies, lines from STDIN to STDOUT until
//! either the End-of-File (EOF) is reached on STDIN (Ctrl+D) or the application
//! is terminated (Ctrl+C).
//!
//! To run the example, start a terminal and navigate to the actix project root
//! and execute:
//!
//! ```text
//! $ cargo run --example echo
//! ```
//!
//! After executing the above command, text can be typed in the terminal and
//! will be written/printed on the next line after pressing the `ENTER` key. The
//! example can be stopped by either sending EOF via Ctrl+D or terminating the
//! process with Ctrl+C.
//!
//! To run the example with logging statements for observation of actor
//! life-cycles, start the example with the following command:
//!
//! ```text
//! $ RUST_LOG=echo=trace cargo run --example echo
//! ```
//!
//! This will print any logging statements within _this_ example. If the
//! `RUST_LOG=echo=trace` is replaced with `RUST_LOG=trace`, then all logging
//! statements within all dependencies and _this_ example will be printed.
//!
//! # Details
//!
//! Three actors are created: (1) [`Stdin`], (2) [`Stdout`], and (3) ['Echo'].
//! The Stdin actor reads lines from STDIN and sends the lines to a recipient.
//! In this case, the recipient is the Echo actor. The Stdin actor handles a
//! stream of decoded bytes from the STDIN thread and converts them to [`Input`]
//! messages that are sent to a recipient. The Stdout actor receives [`Output`]
//! messages from any actor. In this case, the Stdout actor receives messages
//! from the Echo actor. The Stdout actor sends the Output messages to the
//! STDOUT thread via a channel. Finally, the Echo actor handles Input message
//! from the Stdin actor, a.k.a. the recipient, converts the Input messages to
//! Output messages, and sends Output messages to the Stdout actor. Thus, the
//! Echo actor implements the main functionality of echoing, forwarding, and/or
//! copying.
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
//! [`Echo`]: struct.Echo.html
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

/// The message sent from the [`Stdin`] actor to a recipient.
///
/// The Stdin actor publishes (sends) this message to a subscriber (recipient).
/// This message is just a thin wrapper around a series of bytes from STDIN.
///
/// [`Stdin`]: struct.Stdin.html
#[derive(Message)]
struct Input(pub BytesMut);

// A conversion helper implementation for converting BytesMut into the wrapper
// Input message.
impl From<BytesMut> for Input {
    fn from(b: BytesMut) -> Self {
        Input(b)
    }
}

/// The message handled by the [`Stdout`] actor.
///
/// The Stdout actor writes the message, which is just a thin wrapper around a
/// series of bytes, to STDOUT.
///
/// [`Stdout`]: struct.Stdout.html
#[derive(Message)]
struct Output(pub Bytes);

// A conversion helper of an Input message to an Output message so that
// the Echo actor can easily convert the Input message from the Stdin actor to
// the Output message of the Stdout actor.
impl From<Input> for Output {
    fn from(i: Input) -> Self {
        Output(i.0.freeze())
    }
}

/// An actor that reads bytes from STDIN.
///
/// This actor starts a separate thread to avoid blocking and uses a channel to
/// receive a continuous stream of frames from the separate thread. The channel
/// only has one sender (tx), which is moved to the separate thread. When STDIN
/// receives an End-of-File (EOF) signal, the sender will be dropped. This
/// causes the receiver within this actor to become resolved, i.e. completed.
/// The resolution of the receiver stops this actor.
///
/// This actor is implemented as a publisher, in that it does not receive
/// messages from other actors. Instead, it publishes [`Input`] messages to a
/// subscriber (recipient).
///
/// [`Input`]: struct.Input.html
struct Stdin<D> {
    /// The subscriber of the published Input messages.
    recipient: Recipient<Input>,
    /// Decodes the stream of bytes from STDIN into frames.
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
    /// used. This means any codec provided by the tokio crate under the
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
// logging statements to demonstrate the life-cycle of this actor.
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
            // The `io::stdin()` method, which returns an asynchronous Stdin
            // struct, must be used within a tokio runtime to avoid a
            // BlockingError.
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

// The Stdin actor handles the stream of frames from STDIN thread using a
// channel. The receiver is "attached" to the execution context of this actor,
// which means it must implement a stream handler. Using the helper conversion
// for the Input message, i.e. `std::convert::From<BytesMut>`, the frames of
// bytes are easily transformed into Input messages and published/sent to the
// subscriber/recipient.
impl<D> StreamHandler<BytesMut, ()> for Stdin<D>
    where D: Decoder<Item=BytesMut, Error=Error> + Send + Clone + 'static
{
    fn handle(&mut self, item: BytesMut, _ctx: &mut Self::Context) {
        trace!("STDIN stream handle");
        self.recipient.do_send(item.into()).unwrap();
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
    /// The sender of the internal channel used to communicate with the STDOUT
    /// thread.
    ///
    /// When this is dropped, the STDOUT thread will be stopped and the actix
    /// system is shutdown.
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
            // The `io::stdout()` method, which returns an asynchronous Stdout
            // struct, must be used within a tokio runtime to avoid a
            // BlockingError.
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

// Added trace logging statements to help demonstrate the life-cycle of this
// actor.
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

// The Stdout actor handles Output messages.
//
// This is relatively simple. Output messages are sent to the STDOUT thread
// using the internal channel as a series of bytes.
impl Handler<Output> for Stdout {
    type Result = ();

    fn handle(&mut self, msg: Output, _ctx: &mut Context<Self>) {
        trace!("STDOUT handle");
        self.tx.clone().send(msg.0).wait().expect("Send message");
    }
}

// A conversion helper to clean up creation of a Stdout actor from an
// encoder/codec. Uses the current actix system.
impl<E> From<E> for Stdout
    where E: Encoder<Item=Bytes, Error=Error> + Send + Clone + 'static
{
    fn from(e: E) -> Self {
        Stdout::new(System::current(), e)
    }
}

/// An actor that implements the main functionality of the example/application.
///
/// This actor subscribes to Input messages from the [`Stdin`] actor, converts
/// the [`Input`] message into an [`Output`] message, and sends the Output
/// message to the [`Stdout`] actor. In this fashion, the actor echos, forwards,
/// and/or copies the contents from STDIN to STDOUT.
///
/// [`Input`]: struct.Input.html
/// [`Output`]: struct.Output.html
/// [`Stdin`]: struct.Stdin.html
/// [`Stdout`]: struct.Stdout.html
struct Echo {
    /// The address of the [`Stdout`] actor.
    ///
    /// [`Stdout`]: struct.Stdout.html
    stdout: Addr<Stdout>,
}

impl Echo {
    /// Creates a new `Echo` actor.
    ///
    /// Only the address of the [`Stdout`] actor is needed.
    ///
    /// [`Stdout`]: struct.Stdout.html
    pub fn new(stdout: Addr<Stdout>) -> Self {
        Echo {
            stdout: stdout,
        }
    }
}

// Added trace logging statements to demonstrate the life-cycle of this actor.
impl Actor for Echo {
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

// Since the Echo actor is a recipient, or subscriber, of messages from the
// Stdin actor, it must implement a handler for Input messages. The
// implementation is very simple using the conversion helpers and the address of
// the Stdout actor. The Input message is converted to an Output message and
// sent to the Stdout actor.
impl Handler<Input> for Echo {
    type Result = ();

    fn handle(&mut self, item: Input, _ctx: &mut Self::Context) {
        trace!("ECHO handle");
        self.stdout.do_send(item.into())
    }
}

// A conversion helper to create an Echo actor from a Stdout actor.
impl From<Addr<Stdout>> for Echo {
    fn from(s: Addr<Stdout>) -> Self {
        Echo::new(s)
    }
}

/// A wrapper codec for the [`tokio::codec::LinesCodec`].
///
/// Since the `tokio::codec::LinesCodec` decodes and encodes items of the
/// [`String`] type, it cannot be directly used with the [`Stdin`] and
/// [`Stdout`] actors. The `Stdin` and `Stdout` actors use decoders and
/// encoders with [`BytesMut`] and [`Bytes`] items, respectively. Thus, a new
/// codec type that wraps the `tokio::codec::LinesCodec` and handles the
/// conversion of `String` items to `BytesMut` and `Bytes` items is implemented.
/// By wrapping the `tokio::codec::LinesCodec`, the functionality is
/// maintained and the type requirements for using the `Stdin` and `Stdout`
/// actors are met.
///
/// [`Bytes`]: https://carllerche.github.io/bytes/bytes/struct.Bytes.html
/// [`BytesMut`]: https://carllerche.github.io/bytes/bytes/struct.BytesMut.html
/// [`String`]: https://doc.rust-lang.org/std/string/struct.String.html
/// [`Stdin`]: struct.Stdin.html
/// [`Stdout`]: struct.Stdout.html
/// [`tokio::codec::LinesCodec`]: https://tokio-rs.github.io/tokio/tokio/codec/struct.LinesCodec.html
#[derive(Clone)]
struct LinesCodec {
    /// The tokio codec that actually implements reading and writing strings
    /// terminated by an End-of-Line (EOL) marker.
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
    // Initialize the logger.
    //
    // Use `RUST_LOG=echo=trace cargo run --example echo` to display only the
    // logging statements from this example. Running `RUST_LOG=trace cargo run
    // --example echo` will result in logging statements from the entire tokio
    // and mio stack to be displayed.
    env_logger::init();

    // Start the actix system. Note, the Stdin and Stdout actors will
    // automatically create the separate threads to read and writing from STDIN
    // and STDOUT in a non-blocking fashion, respectively. All that is required
    // is to create and start the three actors, Stdin, Stdout, and Echo, in the
    // proper order.
    //
    // Notice because of all of the conversion helpers that are implemented for
    // each actor, running the echo example is essentially on line of code.
    let code = System::run(|| {
        Stdin::new(
            LinesCodec::default(),
            // Because the Stdin and Stdout actors are implemented with generic
            // decoders and encoders, respectively, it is relatively easy to modify
            // this example to read lines of text from STDIN but have STDOUT write
            // the lines in an entirely different format. Simply change the
            // encoder/codec for the Stdout actor.
            Echo::new(Stdout::from(LinesCodec::default()).start()).start().recipient()
        ).start();
    });
    // Exit the example/application with an appropriate exit code. When the
    // example/application successfully terminates, the error code will be zero
    // (0), which is convention. On failure, a non-zero value is used for the
    // exit code.
    std::process::exit(code);
}
