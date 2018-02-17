#![cfg(actix_nightly)]
#![cfg_attr(actix_nightly, feature(proc_macro,))]

extern crate futures;
extern crate actix;
extern crate actix_derive;

use std::io;
use actix_derive::{msg, actor};
use actix::{msgs, Actor, SyncAddress, Arbiter, System};
use futures::{future, Future};

#[msg(usize)]
struct Sum{a: usize, b: usize}

#[msg(usize, io::Error)]
struct Sum1{a: usize, b: usize}

#[msg]
struct Empty;

struct SumActor;

#[actor(Context<_>)]
impl SumActor {

    #[simple(Sum)]
    fn sum(&mut self, a: usize, b: usize) -> usize {
        a + b
    }

    #[handler(Sum1)]
    fn sum1(&mut self, a: usize, b: usize) -> Result<usize, io::Error> {
        Ok(a + b)
    }

    #[simple(Empty)]
    fn empty(&mut self, ctx: &mut actix::Context<Self>) {
        println!("empty");
    }
}

#[test]
fn test_handlers() {
    let system = System::new("test");
    let addr: SyncAddress<_> = SumActor.start();

    system.handle().spawn(addr.call_fut(Sum{a: 10, b: 5}).then(|res| {
        match res {
            Ok(Ok(result)) => assert!(result == 10 + 5),
            _ => panic!("Something went wrong"),
        }
        future::result(Ok(()))
    }));

    system.handle().spawn(addr.call_fut(Sum1{a: 10, b: 5}).then(|res| {
        match res {
            Ok(Ok(result)) => assert!(result == 10 + 5),
            _ => panic!("Something went wrong"),
        }

        Arbiter::system().do_send(msgs::SystemExit(0));
        future::result(Ok(()))
    }));

    system.run();
}
