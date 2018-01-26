extern crate actix;
extern crate futures;
use futures::Future;
use actix::prelude::*;
use actix::actors::{resolver, signal};

#[test]
fn test_resolver() {
    let sys = System::new("test");

    Arbiter::handle().spawn({
        let resolver: LocalAddress<_> = Arbiter::registry().get::<resolver::Connector>();
        resolver.call_fut(
            resolver::Resolve::host("localhost"))
            .then(|addrs| {
                Arbiter::system().send(actix::msgs::SystemExit(0));
                Ok::<_, ()>(())
            })
    });

    Arbiter::handle().spawn({
        let resolver: LocalAddress<_> = Arbiter::registry().get::<resolver::Connector>();

        resolver.call_fut(
            resolver::Connect::host("localhost:5000"))
            .then(|stream| {
                Ok::<_, ()>(())
            })
    });

    sys.run();
}


#[test]
fn test_signal() {
    let sys = System::new("test");

    let _: Address<_> = signal::DefaultSignalsHandler::start_default();
    Arbiter::handle().spawn_fn(move || {
        let sig = Arbiter::system_registry().get::<signal::ProcessSignals>();
        sig.send(Ok(signal::SignalType::Quit));
        Ok(())
    });

    sys.run();
}
