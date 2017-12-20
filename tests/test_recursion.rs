extern crate actix;

use actix::prelude::*;
use actix::constants::MAX_SYNC_POLLS;

struct CounterActor;

struct Count(u32);

// Maximum number of items to keep alive
const MAX_ITEMS: u32 = MAX_SYNC_POLLS;

// Number of items to keep alive
static mut N_ITEMS: u32 = 0;

// Keeps track of instances alive
struct TrackableItem;

impl TrackableItem {
    fn new() -> TrackableItem {
        unsafe {
            N_ITEMS += 1;
        }
        TrackableItem {}
    }

    fn count() -> u32 {
        unsafe {
            N_ITEMS
        }
    }
}

impl Drop for TrackableItem {
    fn drop(&mut self) {
        unsafe {
            N_ITEMS -= 1;
        }
    }
}

impl ResponseType for Count {
    type Item = TrackableItem;
    type Error = ();
}

impl Actor for CounterActor {
    type Context = Context<Self>;
}

impl Handler<Count> for CounterActor {
    fn handle(
        &mut self,
        msg: Count,
        ctx: &mut Self::Context,
    ) -> Response<Self, Count> {

        assert!(TrackableItem::count() <= MAX_ITEMS);

        // send a message to self,
        // creating sorta async recursion
        let my_address: Address<CounterActor> = ctx.address();

        if msg.0 < MAX_ITEMS * 2 {
            my_address.send(Count(msg.0 + 1));
        } else {
            Arbiter::system().send(msgs::SystemExit(0));
        }

        Self::reply(TrackableItem::new())
    }
}

// When actor sends messages to itself recursively,
// results of the Handler should not stack up indefinitely
#[test]
fn test_recursion() {
    let system = actix::System::new("test");
    let addr: Address<_> = CounterActor.start();
    addr.send(Count(0));
    system.run();
}
