extern crate actix;
extern crate futures;
extern crate tokio;
use actix::actors::{self, signal};
use actix::prelude::*;
use futures::Future;

#[test]
fn test_resolver() {
    System::run(|| {
        tokio::spawn({
            let resolver = Arbiter::registry().get::<actors::Connector>();
            resolver.send(actors::Resolve::host("localhost")).then(|_| {
                Arbiter::system().do_send(actix::msgs::SystemExit(0));
                Ok::<_, ()>(())
            })
        });

        tokio::spawn({
            let resolver = Arbiter::registry().get::<actors::Connector>();
            resolver
                .send(actors::Connect::host("localhost:5000"))
                .then(|_| Ok::<_, ()>(()))
        });
    });
}

#[test]
#[cfg(unix)]
fn test_signal() {
    System::run(|| {
        let _addr = signal::DefaultSignalsHandler::start_default();
        let sig = Arbiter::registry().get::<signal::ProcessSignals>();

        // send SIGTERM
        std::thread::spawn(move || {
            // emulate SIGNTERM
            sig.do_send(signal::SignalType::Quit);
        });
    });
}

#[test]
#[cfg(unix)]
fn test_signal_term() {
    System::run(|| {
        let _addr = signal::DefaultSignalsHandler::start_default();
        tokio::spawn(futures::lazy(move || {
            let sig = Arbiter::registry().get::<signal::ProcessSignals>();
            sig.do_send(signal::SignalType::Term);
            Ok(())
        }));
    });
}

#[test]
#[cfg(unix)]
fn test_signal_int() {
    System::run(|| {
        let _addr = signal::DefaultSignalsHandler::start_default();
        tokio::spawn(futures::lazy(move || {
            let sig = Arbiter::registry().get::<signal::ProcessSignals>();
            sig.do_send(signal::SignalType::Hup);
            sig.do_send(signal::SignalType::Int);
            Ok(())
        }));
    });
}
