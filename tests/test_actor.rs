extern crate actix;
extern crate futures;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use actix::prelude::*;

#[derive(Debug)]
struct Num(usize);

impl ResponseType for Num {
    type Item = ();
    type Error = ();
}

struct MyActor(Arc<AtomicUsize>, Arc<AtomicBool>);

impl Actor for MyActor {
    type Context = actix::Context<Self>;

    fn stopping(&mut self, _: &mut Self::Context) -> bool {
        println!("SEND");
        Arbiter::system().send(actix::msgs::SystemExit(0));
        true
    }
}

impl actix::Handler<Result<Num, ()>> for MyActor {
    type Result = ();

    fn handle(&mut self, msg: Result<Num, ()>, _: &mut actix::Context<MyActor>) {
        if let Ok(msg) = msg {
            self.0.store(self.0.load(Ordering::Relaxed) + msg.0, Ordering::Relaxed);
        } else {
            self.1.store(true, Ordering::Relaxed);
        }
    }
}

#[test]
fn test_stream() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));
    let err = Arc::new(AtomicBool::new(false));
    let items = vec![Num(1), Num(1), Num(1), Num(1), Num(1), Num(1), Num(1)];

    let act_count = Arc::clone(&count);
    MyActor::create::<(), _>(move |ctx| {
        ctx.add_stream(futures::stream::iter_ok::<_, ()>(items));
        MyActor(act_count, err)
    });

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 7);
}

#[test]
fn test_stream_with_error() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));
    let error = Arc::new(AtomicBool::new(false));
    let items = vec![Ok(Num(1)), Ok(Num(1)), Err(()), Ok(Num(1)),
                     Ok(Num(1)), Ok(Num(1)), Ok(Num(1)), Ok(Num(1))];

    let act_count = Arc::clone(&count);
    let act_error = Arc::clone(&error);
    MyActor::create::<(), _>(move |ctx| {
        ctx.add_stream(futures::stream::iter_result(items));
        MyActor(act_count, act_error)
    });

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 7);
    assert!(error.load(Ordering::Relaxed));
}
