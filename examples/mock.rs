extern crate actix;
extern crate futures;

use actix::actors::mocker::Mocker;
use actix::prelude::*;
use futures::Future;

#[derive(Eq, PartialEq, Debug)]
struct DoStuff {}

impl Message for DoStuff {
    type Result = String;
}

/// An example an actor which you would not like to actually run in a test
struct DBClientActor {}

impl Actor for DBClientActor {
    type Context = Context<Self>;
}

/// Nothing special needs to be done here
impl Handler<DoStuff> for DBClientActor {
    type Result = String;

    fn handle(&mut self, msg: DoStuff, _: &mut Context<Self>) -> Self::Result {
        // do normal database stuff here
        "Reply from database".to_string()
    }
}

/// Required for SystemService
impl Supervised for DBClientActor {}
impl Default for DBClientActor {
    fn default() -> Self {
        DBClientActor {}
    }
}

/// Using a SystemService allows the mocker to be injected into the entire
/// application
impl SystemService for DBClientActor {
    fn service_started(&mut self, _ctx: &mut Context<Self>) {}
}

/// This wraps the type you wish to test with Mocker<T> which allows you to
/// mock it, but only when you are testing
#[cfg(not(test))]
type DBClientAct = DBClientActor;
#[cfg(test)]
type DBClientAct = Mocker<DBClientActor>;

/// This is an example of a function you would like to test that calls external
/// actors which would be hard to test otherwise
fn example_function() {
    let res = DBClientAct::from_registry().send(DoStuff {});

    Arbiter::spawn(
        res.map(|res| {
            println!("got reply {}", res);
            Arbiter::system().do_send(actix::msgs::SystemExit(0));
        }).map_err(|_| ()),
    );
}

fn main() {
    let system = System::new("test");

    // call it normally
    example_function();

    system.run();
}

#[test]
fn test_stuff() {
    let system = System::new("test");

    // here we setup the mocking
    let _: Addr<Syn, _> = DBClientAct::init_actor(|_| {
        DBClientAct::mock(Box::new(|msg, ctx| {
            // asserting the input to the actor is as expected
            assert_eq!(msg.downcast_ref::<DoStuff>(), Some(&DoStuff {}));

            // replying
            Box::new(Some("Reply from mocker".to_string()))
        }))
    });

    example_function();

    system.run();
}
