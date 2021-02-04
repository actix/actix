use std::pin::Pin;
use std::task::{Context, Poll};

use actix::io::SinkWrite;
use actix::prelude::*;
use bytes::{Buf, Bytes};
use futures_sink::Sink;
use tokio::sync::mpsc;

type ByteSender = mpsc::UnboundedSender<u8>;

struct MySink {
    sender: ByteSender,
    queue: Vec<Bytes>,
}

// simple sink that send one bit at a time
// and produce an error on '#'
impl Sink<Bytes> for MySink {
    type Error = ();

    fn poll_ready(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();

        if !this.queue.is_empty() {
            let bytes = &mut this.queue[0];
            if bytes[0] == b'#' {
                return Poll::Ready(Err(()));
            }

            this.sender.send(bytes[0]).unwrap();
            bytes.advance(1);
            if bytes.is_empty() {
                this.queue.remove(0);
            }
        }

        if this.queue.is_empty() {
            Poll::Ready(Ok(()))
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    fn start_send(self: Pin<&mut Self>, bytes: Bytes) -> Result<(), Self::Error> {
        self.get_mut().queue.push(bytes);
        Ok(())
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.poll_ready(cx)
    }

    fn poll_close(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(), Self::Error>> {
        self.poll_ready(cx)
    }
}

struct SinkUnboundedSender {
    tx: mpsc::UnboundedSender<Bytes>,
}

impl Sink<Bytes> for SinkUnboundedSender {
    type Error = mpsc::error::SendError<Bytes>;

    fn poll_ready(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, t: Bytes) -> Result<(), Self::Error> {
        mpsc::UnboundedSender::send(&self.get_mut().tx, t)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        _: &mut Context,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

struct Data {
    bytes: Bytes,
    last: bool,
}

impl Message for Data {
    type Result = ();
}

struct MyActor {
    sink: SinkWrite<Bytes, MySink>,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;
}

impl actix::io::WriteHandler<()> for MyActor {
    fn finished(&mut self, _ctxt: &mut Self::Context) {
        System::current().stop();
    }
}

impl Handler<Data> for MyActor {
    type Result = ();
    fn handle(&mut self, data: Data, _ctxt: &mut Self::Context) {
        let _ = self.sink.write(data.bytes);
        if data.last {
            self.sink.close();
        }
    }
}

#[actix::test]
async fn test_send_1() {
    let (sender, mut receiver) = mpsc::unbounded_channel();

    actix_rt::spawn(async move {
        let addr = MyActor::create(move |ctxt| {
            let sink = MySink {
                sender,
                queue: Vec::new(),
            };
            MyActor {
                sink: SinkWrite::new(sink, ctxt),
            }
        });

        let data = Data {
            bytes: Bytes::from_static(b"Hello"),
            last: true,
        };

        addr.do_send(data);
    });

    let mut res = Vec::new();
    while let Some(r) = receiver.recv().await {
        res.push(r);
    }

    assert_eq!(b"Hello", &res[..]);
}

#[actix::test]
async fn test_send_2() {
    let (sender, mut receiver) = mpsc::unbounded_channel();

    actix_rt::spawn(async move {
        let addr = MyActor::create(move |ctxt| {
            let sink = MySink {
                sender,
                queue: Vec::new(),
            };
            MyActor {
                sink: SinkWrite::new(sink, ctxt),
            }
        });

        let data = Data {
            bytes: Bytes::from_static(b"Hello"),
            last: false,
        };

        addr.do_send(data);

        let data = Data {
            bytes: Bytes::from_static(b" world"),
            last: true,
        };

        addr.do_send(data);
    });

    let mut res = Vec::new();
    while let Some(r) = receiver.recv().await {
        res.push(r);
    }

    assert_eq!(b"Hello world", &res[..]);
}

#[actix::test]
async fn test_send_error() {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    actix_rt::spawn(async move {
        let addr = MyActor::create(move |ctxt| {
            let sink = MySink {
                sender,
                queue: Vec::new(),
            };
            MyActor {
                sink: SinkWrite::new(sink, ctxt),
            }
        });

        let data = Data {
            bytes: Bytes::from_static(b"Hello #"),
            last: false,
        };

        addr.do_send(data);
    });

    let mut res = Vec::new();
    while let Some(r) = receiver.recv().await {
        res.push(r);
    }

    assert_eq!(b"Hello ", &res[..]);
}

struct AnotherActor {
    sink: SinkWrite<Bytes, SinkUnboundedSender>,
}

impl Actor for AnotherActor {
    type Context = actix::Context<Self>;
}

impl<T> actix::io::WriteHandler<mpsc::error::SendError<T>> for AnotherActor {
    fn finished(&mut self, _ctxt: &mut Self::Context) {
        System::current().stop();
    }
}

impl Handler<Data> for AnotherActor {
    type Result = ();
    fn handle(&mut self, data: Data, _ctxt: &mut Self::Context) {
        let _ = self.sink.write(data.bytes);

        if data.last {
            self.sink.close();
        }
    }
}

#[actix::test]
async fn test_send_bytes() {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let bytes = Bytes::from_static(b"Hello");
    let expected_bytes = bytes.clone();

    actix_rt::spawn(async move {
        let addr = AnotherActor::create(move |ctxt| AnotherActor {
            sink: SinkWrite::new(SinkUnboundedSender { tx: sender }, ctxt),
        });

        let data = Data { bytes, last: true };

        addr.do_send(data);
    });

    let res = receiver.recv().await.unwrap();
    assert_eq!(expected_bytes, res);
}
