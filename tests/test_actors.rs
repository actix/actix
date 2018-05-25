extern crate actix;
extern crate futures;
use actix::actors::{self, signal};
use actix::prelude::*;
use futures::Future;

#[test]
fn test_resolver() {
    let sys = System::new("test");

    Arbiter::spawn({
        let resolver: Addr<Unsync, _> = Arbiter::registry().get::<actors::Connector>();
        resolver.send(actors::Resolve::host("localhost")).then(|_| {
            Arbiter::system().do_send(actix::msgs::SystemExit(0));
            Ok::<_, ()>(())
        })
    });

    Arbiter::spawn({
        let resolver: Addr<Unsync, _> = Arbiter::registry().get::<actors::Connector>();

        resolver
            .send(actors::Connect::host("localhost:5000"))
            .then(|_| Ok::<_, ()>(()))
    });

    sys.run();
}

//#[test]
#[cfg(unix)]
fn test_signal() {
    let sys = System::new("test");
    let _: Addr<Syn, _> = signal::DefaultSignalsHandler::start_default();
    Arbiter::spawn_fn(move || {
        let sig = Arbiter::system_registry().get::<signal::ProcessSignals>();
        sig.do_send(signal::SignalType::Quit);
        Ok(())
    });
    sys.run();
}

// #[test]
#[cfg(unix)]
fn test_signal_term() {
    let sys = System::new("test");
    let _: Addr<Syn, _> = signal::DefaultSignalsHandler::start_default();
    Arbiter::spawn_fn(move || {
        let sig = Arbiter::system_registry().get::<signal::ProcessSignals>();
        sig.do_send(signal::SignalType::Term);
        Ok(())
    });
    sys.run();
}

//#[test]
#[cfg(unix)]
fn test_signal_int() {
    let sys = System::new("test");
    let _: Addr<Syn, _> = signal::DefaultSignalsHandler::start_default();
    Arbiter::spawn_fn(move || {
        let sig = Arbiter::system_registry().get::<signal::ProcessSignals>();
        sig.do_send(signal::SignalType::Hup);
        sig.do_send(signal::SignalType::Int);
        Ok(())
    });
    sys.run();
}
