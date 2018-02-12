extern crate actix;
extern crate futures;
use futures::Future;
use actix::prelude::*;
use actix::actors::{self, signal};


#[test]
fn test_resolver() {
    let sys = System::new("test");

    Arbiter::handle().spawn({
        let resolver: Addr<Unsync<_>> = Arbiter::registry().get::<actors::Connector>();
        resolver.call_fut(
            actors::Resolve::host("localhost"))
            .then(|_| {
                Arbiter::system().send(actix::msgs::SystemExit(0));
                Ok::<_, ()>(())
            })
    });

    Arbiter::handle().spawn({
        let resolver: Addr<Unsync<_>> = Arbiter::registry().get::<actors::Connector>();

        resolver.call_fut(
            actors::Connect::host("localhost:5000"))
            .then(|_| {
                Ok::<_, ()>(())
            })
    });

    sys.run();
}


#[test]
#[cfg(unix)]
fn test_signal() {
    let sys = System::new("test");
    let _: Addr<Syn<_>> = signal::DefaultSignalsHandler::start_default();
    Arbiter::handle().spawn_fn(move || {
        let sig = Arbiter::system_registry().get::<signal::ProcessSignals>();
        sig.send(signal::SignalType::Quit);
        Ok(())
    });
    sys.run();
}

#[test]
#[cfg(unix)]
fn test_signal_term() {
    let sys = System::new("test");
    let _: Addr<Syn<_>> = signal::DefaultSignalsHandler::start_default();
    Arbiter::handle().spawn_fn(move || {
        let sig = Arbiter::system_registry().get::<signal::ProcessSignals>();
        sig.send(signal::SignalType::Term);
        Ok(())
    });
    sys.run();
}

#[test]
#[cfg(unix)]
fn test_signal_int() {
    let sys = System::new("test");
    let _: Addr<Syn<_>> = signal::DefaultSignalsHandler::start_default();
    Arbiter::handle().spawn_fn(move || {
        let sig = Arbiter::system_registry().get::<signal::ProcessSignals>();
        sig.send(signal::SignalType::Hup);
        sig.send(signal::SignalType::Int);
        Ok(())
    });
    sys.run();
}
