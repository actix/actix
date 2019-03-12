use actix::actors::resolver;
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
