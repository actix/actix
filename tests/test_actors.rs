use actix::actors::resolver;
use actix::prelude::*;

#[actix_rt::test]
async fn test_resolver() {
    // FIXME: disabled waiting for trust dns support tokio 0.3
    // Arbiter::spawn(async {
    //     let resolver = resolver::Resolver::from_registry();
    //     let _ = resolver
    //         .send(resolver::Resolve::host("localhost"))
    //         .await
    //         .unwrap();
    // });
    //
    // let resolver = resolver::Resolver::from_registry();
    // let _ = resolver
    //     .send(resolver::Connect::host("localhost:5000"))
    //     .await
    //     .unwrap();
}
