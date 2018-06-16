extern crate actix;
extern crate futures;
extern crate tokio;

use std::thread;
use std::time::Duration;

use actix::actors::{resolver, signal};
use actix::prelude::*;
use futures::Future;

#[test]
fn test_resolver() {
    System::run(|| {
        Arbiter::spawn({
            let resolver = System::current().registry().get::<resolver::Resolver>();
            resolver
                .send(resolver::Resolve::host("localhost"))
                .then(|_| {
                    System::current().stop();
                    Ok::<_, ()>(())
                })
        });

        Arbiter::spawn({
            let resolver = System::current().registry().get::<resolver::Resolver>();
            resolver
                .send(resolver::Connect::host("localhost:5000"))
                .then(|_| Ok::<_, ()>(()))
        });
    });
}

#[test]
#[cfg(unix)]
fn test_signal() {
    System::run(|| {
        let _addr = signal::DefaultSignalsHandler::start_default();
        let sig = System::current().registry().get::<signal::ProcessSignals>();

        // send SIGTERM
        std::thread::spawn(move || {
            // we need this because DefaultSignalsHandler starts a bit later
            thread::sleep(Duration::from_millis(100));

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
        Arbiter::spawn(futures::lazy(move || {
            let sig = System::current().registry().get::<signal::ProcessSignals>();
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
        Arbiter::spawn(futures::lazy(move || {
            let sig = System::current().registry().get::<signal::ProcessSignals>();
            sig.do_send(signal::SignalType::Hup);
            sig.do_send(signal::SignalType::Int);
            Ok(())
        }));
    });
}
