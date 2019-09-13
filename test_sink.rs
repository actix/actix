//use actix::io::SinkWrite;
use actix::prelude::*;
use bytes::Bytes;
use futures::sink::Sink;
use futures::channel::mpsc;

type ByteSender = mpsc::UnboundedSender<u8>;

struct MySink {
    sender: ByteSender,
    queue: Vec<Bytes>,
}

/*
// simple sink that send one bit at a time
// and produce an error on '#'
impl Sink<Bytes> for MySink {

    type Error = ();

    fn start_send(
        &mut self,
        bytes: Bytes,
    ) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.queue.push(bytes);
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        if !self.queue.is_empty() {
            let bytes = &mut self.queue[0];
            if bytes[0] == b'#' {
                return Err(());
            }

            self.sender.unbounded_send(bytes[0]).unwrap();
            bytes.split_to(1);
            if bytes.len() == 0 {
                self.queue.remove(0);
            }
        }
        if self.queue.is_empty() {
            Ok(Async::Ready(()))
        } else {
            futures::task::current().notify();
            Ok(Async::NotReady)
        }
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
    sink: SinkWrite<MySink>,
}

impl Actor for MyActor {
    type Context = actix::Context<Self>;
}

impl actix::io::WriteHandler<()> for MyActor {
    fn finished(&mut self, _ctxt: &mut Self::Context) {
        System::current().stop();
    }
}
*/

impl Handler<Data> for MyActor {
    type Result = ();
    fn handle(&mut self, data: Data, _ctxt: &mut Self::Context) {
        self.sink.write(data.bytes).unwrap();
        if data.last {
            self.sink.close();
        }
    }
}

#[test]
fn test_send_1() {
    let (sender, receiver) = mpsc::unbounded();
    actix::System::run(move || {
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
    })
    .unwrap();
    /*
    todo: FIX
    let res = receiver.collect().wait();
    let res = res.unwrap();
    assert_eq!(b"Hello", &res[..]);
    */
}

#[test]
fn test_send_2() {
    let (sender, receiver) = mpsc::unbounded();
    actix::System::run(move || {
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
    })
    .unwrap();

    /*
    TODO: fix
    let res = receiver.collect().wait();
    let res = res.unwrap();
    assert_eq!(b"Hello world", &res[..]);
    */
}

#[test]
fn test_send_error() {
    let (sender, receiver) = mpsc::unbounded();
    actix::System::run(move || {
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
    })
    .unwrap();

    /*
    TODO: fix
    let res = receiver.collect().wait();
    let res = res.unwrap();
    assert_eq!(b"Hello ", &res[..]);

    */
}

type BytesSender = mpsc::UnboundedSender<Bytes>;

struct AnotherActor {
    sink: SinkWrite<BytesSender>,
}

impl Actor for AnotherActor {
    type Context = actix::Context<Self>;
}
/*
impl actix::io::WriteHandler<mpsc::SendError<Bytes>> for AnotherActor {
    fn finished(&mut self, _ctxt: &mut Self::Context) {
        System::current().stop();
    }
}
*/
impl Handler<Data> for AnotherActor {
    type Result = ();
    fn handle(&mut self, data: Data, _ctxt: &mut Self::Context) {
        self.sink.write(data.bytes).unwrap();
        if data.last {
            self.sink.close();
        }
    }
}

#[test]
fn test_send_bytes() {
    let (sender, receiver) = mpsc::unbounded();
    let bytes = Bytes::from_static(b"Hello");
    let expected_bytes = bytes.clone();
    actix::System::run(move || {
        let addr = AnotherActor::create(move |ctxt| AnotherActor {
            sink: SinkWrite::new(sender, ctxt),
        });

        let data = Data { bytes, last: true };

        addr.do_send(data);
    })
    .unwrap();

    /*
    TODO: Fix
    let mut iter = receiver.wait();
    let res = iter.next().unwrap().unwrap();
    assert_eq!(expected_bytes, res);
    */
}
