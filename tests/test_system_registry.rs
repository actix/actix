use actix::prelude::*;
use actix::SystemRegistry;

#[derive(Default)]
struct MyActor;

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl SystemService for MyActor {}

impl Supervised for MyActor {}

#[test]
#[should_panic(expected = "Actor already started")]
fn test_unchecked_set_panics() {
    System::run(|| {
        let arbiter = Arbiter::new();

        let addr = MyActor::start_default();

        SystemRegistry::set(addr);

        let addr = MyActor::from_registry();

        SystemRegistry::set(addr);

        System::current().stop();
    })
    .unwrap();
}

#[test]
fn test_checked_set_does_not_panic() {
    System::run(|| {
        let arbiter = Arbiter::new();

        let addr = MyActor::start_default();

        SystemRegistry::set(addr);

        let addr = MyActor::from_registry();

        SystemRegistry::set_checked(addr);

        System::current().stop();
    })
    .unwrap()
}
