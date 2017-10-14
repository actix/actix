#![cfg(feature = "use_std")]

extern crate actix;
extern crate futures;

use futures::prelude::*;
use futures::unsync::oneshot;
use futures::future::lazy;
use futures::stream::{iter_ok, unfold};
use actix::queue::unsync::{self as mpsc, SendError};


#[test]
fn mpsc_send_recv() {
    let (tx, rx) = mpsc::channel::<i32>(1);
    let mut rx = rx.wait();

    tx.send(42).wait().unwrap();

    assert_eq!(rx.next(), Some(Ok(42)));
    assert_eq!(rx.next(), None);
}

#[test]
fn mpsc_rx_notready() {
    let (_tx, mut rx) = mpsc::channel::<i32>(1);

    lazy(|| {
        assert_eq!(rx.poll().unwrap(), Async::NotReady);
        Ok(()) as Result<(), ()>
    }).wait().unwrap();
}

#[test]
fn mpsc_rx_end() {
    let (_, mut rx) = mpsc::channel::<i32>(1);

    lazy(|| {
        assert_eq!(rx.poll().unwrap(), Async::Ready(None));
        Ok(()) as Result<(), ()>
    }).wait().unwrap();
}

#[test]
fn mpsc_tx_notready() {
    let (tx, _rx) = mpsc::channel::<i32>(1);
    let tx = tx.send(1).wait().unwrap();
    lazy(move || {
        assert!(tx.send(2).poll().unwrap().is_not_ready());
        Ok(()) as Result<(), ()>
    }).wait().unwrap();
}

#[test]
fn mpsc_tx_err() {
    let (tx, _) = mpsc::channel::<i32>(1);
    lazy(move || {
        assert!(tx.send(2).poll().is_err());
        Ok(()) as Result<(), ()>
    }).wait().unwrap();
}

#[test]
fn mpsc_backpressure() {
    let (tx, rx) = mpsc::channel::<i32>(1);
    lazy(move || {
        iter_ok(vec![1, 2, 3])
            .forward(tx)
            .map_err(|e: SendError<i32>| panic!("{}", e))
            .join(rx.take(3).collect().map(|xs| {
                assert_eq!(xs, [1, 2, 3]);
            }))
    }).wait().unwrap();
}

#[test]
fn mpsc_unbounded() {
    let (tx, rx) = mpsc::unbounded::<i32>();
    lazy(move || {
        iter_ok(vec![1, 2, 3])
            .forward(tx)
            .map_err(|e: SendError<i32>| panic!("{}", e))
            .join(rx.take(3).collect().map(|xs| {
                assert_eq!(xs, [1, 2, 3]);
            }))
    }).wait().unwrap();
}

#[test]
fn mpsc_recv_unpark() {
    let core = Core::new();
    let (tx, rx) = mpsc::channel::<i32>(1);
    let tx2 = tx.clone();
    core.spawn(rx.collect().map(|xs| assert_eq!(xs, [1, 2])));
    core.spawn(lazy(move || tx.send(1).map(|_| ()).map_err(|e| panic!("{}", e))));
    core.run(lazy(move || tx2.send(2))).unwrap();
}

#[test]
fn mpsc_send_unpark() {
    let core = Core::new();
    let (tx, rx) = mpsc::channel::<i32>(1);
    let (donetx, donerx) = oneshot::channel();
    core.spawn(iter_ok(vec![1, 2]).forward(tx)
        .then(|x: Result<_, SendError<i32>>| {
            assert!(x.is_err());
            donetx.send(()).unwrap();
            Ok(())
        }));
    core.spawn(lazy(move || { let _ = rx; Ok(()) }));
    core.run(donerx).unwrap();
}

#[test]
fn spawn_sends_items() {
    let core = Core::new();
    let stream = unfold(0, |i| Some(Ok::<_,u8>((i, i + 1))));
    let rx = mpsc::spawn(stream, &core, 1);
    assert_eq!(core.run(rx.take(4).collect()).unwrap(),
               [0, 1, 2, 3]);
}
