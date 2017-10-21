extern crate actix;
extern crate futures;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use actix::prelude::*;

#[derive(Debug)]
struct Num(usize);

impl ResponseType for Num {
    type Item = ();
    type Error = ();
}

struct MyActor(Arc<AtomicUsize>);

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl StreamHandler<Num> for MyActor {
    fn finished(&mut self, ctx: &mut Self::Context) {
        Arbiter::system().send(msgs::SystemExit(0));
    }
}

impl Handler<Num> for MyActor {

    fn handle(&mut self, msg: Num, _: &mut Context<MyActor>) -> Response<Self, Num> {
        self.0.store(self.0.load(Ordering::Relaxed) + msg.0, Ordering::Relaxed);
        Self::empty()
    }
}

#[test]
fn test_add_stream() {
    let sys = System::new("test");
    let count = Arc::new(AtomicUsize::new(0));
    let items = vec![Num(1), Num(1), Num(1), Num(1), Num(1), Num(1), Num(1)];

    let act_count = Arc::clone(&count);
    MyActor::create::<(), _>(move |ctx| {
        ctx.add_stream(futures::stream::iter_ok::<_, ()>(items));
        MyActor(act_count)
    });

    sys.run();
    assert_eq!(count.load(Ordering::Relaxed), 7);
}
