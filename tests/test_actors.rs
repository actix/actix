use actix::actors::resolver;
use actix::prelude::*;

// #[test]
// fn test_resolver() {
//     System::run(|| {
//         Arbiter::spawn(async {
//             let resolver = resolver::Resolver::from_registry();
//             let _ = resolver.send(resolver::Resolve::host("localhost")).await;
//             System::current().stop();
//         });

//         Arbiter::spawn(async {
//             let resolver = resolver::Resolver::from_registry();
//             let _ = resolver
//                 .send(resolver::Connect::host("localhost:5000"))
//                 .await;
//         });
//     })
//     .unwrap();
// }
