extern crate actix;

use actix::prelude::*;

struct CounterActor;

struct Count(u32);

// Maximum number of items to keep alive
const MAX_ITEMS: u32 = 256;

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
        unsafe { N_ITEMS }
    }
}

impl Drop for TrackableItem {
    fn drop(&mut self) {
        unsafe {
            N_ITEMS -= 1;
        }
    }
}

impl Message for Count {
    type Result = TrackableItem;
}

impl Actor for CounterActor {
    type Context = Context<Self>;
}

impl Handler<Count> for CounterActor {
    type Result = MessageResult<Count>;

    fn handle(&mut self, msg: Count, ctx: &mut Self::Context) -> Self::Result {
        assert!(TrackableItem::count() <= MAX_ITEMS);

        // send a message to self,
        // creating sorta async recursion
        let my_address = ctx.address();

        my_address.do_send(Count(msg.0 + 1));
        MessageResult(TrackableItem::new())
    }
}

// When actor sends messages to itself recursively,
// results of the Handler should not stack up indefinitely
#[test]
#[should_panic]
fn test_recursion() {
    actix::System::run(|| {
        let addr = CounterActor.start();
        addr.do_send(Count(0));
    });
}
