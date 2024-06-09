# CHANGES

## Unreleased

## 0.13.5

- Add `Registry::try_get()` method.
- Add `Registry::get_or_start_default()` method.
- Deprecate `Registry::get()` method.
- Relax `A: ArbiterService` bound on `Registry::query()`.
- Relax `A: ArbiterService` bound on `Registry::set()`.

## 0.13.4

- Add `AsyncContext::run_interval_at()` method.

## 0.13.3

- No significant changes since `0.13.2`.

## 0.13.2

- Expose `ContextFut::restart`.
- Unquoted types are now allowed in the `#[rtype(result = TYPE)]` position when type is a path.

## 0.13.1

### Added

- Add `SyncArbiter::start_with_thread_builder()`.
- Derive `PartialEq` and `Eq` for `MailboxError`.
- Minimum supported Rust version (MSRV) is now 1.68.

### Fixed

- Fix `SinkWrite` sometimes not sending all messages in its buffer.

## 0.13.0

### Added

- Add `Sender::downgrade` trait method. [#518]
- Add `Recipient::downgrade` method for obtaining a `WeakRecipient`. [#518]
- Expose `WeakSender` trait. [#518]
- Implement `Clone` for `WeakRecipient`. [#518]
- Implement `From<Recipient>` for `WeakRecipient`. [#518]
- Implement `From<Pin<Box<dyn ActorFuture>>>` for `ActorResponse`. [#509]
- Implement `PartialEq` and `Eq` for `WeakAddr`. [#523]
- Implement `PartialEq` and `Eq` for `WeakAddressSender`. [#523]
- Implement `From<Recipient>` for `WeakRecipient`. [#518]

### Changed

- `Recipient::do_send()` no longer has a return value. [#441]
- Updated `tokio-util` dependency to `0.7`. [#525]
- Updated minimum supported Rust version to 1.54.

### Removed

- Remove `Resolver` actor. [#451]

[#441]: https://github.com/actix/actix/pull/441
[#451]: https://github.com/actix/actix/pull/451
[#509]: https://github.com/actix/actix/pull/509
[#518]: https://github.com/actix/actix/pull/518
[#525]: https://github.com/actix/actix/pull/525

## 0.12.0

### Added

- Add `fut::try_future::ActorTryFuture`. [#419]
- Add `fut::try_future::ActorTryFutureExt` trait with `map_ok`, `map_err` and `and_then` combinator. [#419]
- Add `fut::future::ActorFutureExt::boxed_local` [#493]
- Implemented `MessageResponse` for `Vec<T>` [#501]

### Changed

- Make `Context::new` public. [#491]
- `SinkWriter::write` returns `Result` instead of `Option`. [#499]

[#419]: https://github.com/actix/actix/pull/419
[#491]: https://github.com/actix/actix/pull/491
[#493]: https://github.com/actix/actix/pull/493
[#499]: https://github.com/actix/actix/pull/499
[#501]: https://github.com/actix/actix/pull/501

## 0.11.1

### Fixed

- Panics caused by instant cancellation of a spawned future. [#484]

[#484]: https://github.com/actix/actix/pull/484

## 0.11.0

### Removed

- Remove `fut::IntoActorFuture` trait. [#475]
- Remove `fut::future::WrapFuture`'s `Output` associated type. [#475]
- Remove `fut::stream::WrapStream`'s `Item` associated type. [#475]
- Remove `prelude::Future` re-export from std. [#482]
- Remove `fut::future::Either` re-export. Support for the enum re-exported from `futures_util` enum still exists. [#482]
- Remove `fut::future::FutureResult` type alias. [#482]

[#475]: https://github.com/actix/actix/pull/475
[#479]: https://github.com/actix/actix/pull/479
[#482]: https://github.com/actix/actix/pull/482

## 0.11.0-beta.3

### Added

- Added `fut::{ActorFutureExt, ActorStreamExt}` traits for extension method for `ActorFuture` and `ActorStream` trait. This is aiming to have a similar traits set inline with `futures` crate. [#474]
- Added `ActorStreamExt::collect` method for collect an actor stream's items and output them as an actor future. [#474]
- Added `ActorStreamExt::take_while` method to take an actor stream's items based on the closure output. [#474]
- Added `ActorStreamExt::skip_while` method to skip an actor stream's items based on the closure output. [#474]
- Added `fut::LocalBoxActorFuture` type to keep inline with the `futures::future::LocalBoxFuture` type. [#474]

### Changed

- Rework `fut::future::ActorFuture` trait. [#465]
- `fut::future::{wrap_future, wrap_stream}` would need type annotation for Actor type. [#465]
- `dev::MessageResponse::handle` method does not need generic type [#472]
- `fut::{ok, err, result, FutureResult, Either}` are changed to re-export of `futures::future::{ready, Ready, Either}` types. [#474]
- `futures::future::{ok, err, ready, Ready, Either}` types impls `ActorFuture` trait by default. [#474]

### Removed

- Remove `dev::ResponseChannel` trait. [#472]

[#465]: https://github.com/actix/actix/pull/465
[#472]: https://github.com/actix/actix/pull/472
[#474]: https://github.com/actix/actix/pull/474

## 0.11.0-beta.2

### Changed

- Update `actix-rt` to `v2.0.0`. [#461]
- Feature `resolver` is no longer default. [#461]
- Rename `derive` feature to `macros` since it now includes derive \*and- attribute macros. [#461]

[#421]: https://github.com/actix/actix/pull/421

## 0.11.0-beta.1

### Added

- Re-export `actix_rt::main` macro as `actix::main`. [#448]
- Added `actix::fut::Either::{left, right}()` variant constructors. [#453]

### Changed

- The re-exported `actix-derive` macros are now conditionally included with the `derive` feature which is enabled by default but can be switched off to reduce dependencies. [#424]
- The `where` clause on `Response::fut()` was relaxed to no longer require `T: Unpin`, allowing a `Response` to be created with an `async` block [#421]
- Allow creating `WeakRecipient` from `WeakAddr`, similar to `Recipient` from `Addr`. [#432]
- Send `SyncArbiter` to current `System`'s `Arbiter` and run it as future there. Enables nested `SyncArbiter`s. [#439]
- Use generic type instead of associate type for `EnvelopeProxy`. [#445]
- `SyncEnvelopeProxy` and `SyncContextEnvelope` are no longer bound to an Actor. [#445]
- Rename `actix::clock::{delay_for, delay_until, Delay}` to `{sleep, sleep_until, Sleep}`. [#443]
- Remove all `Unpin` requirement from `ActorStream`. [#443]
- Update examples and tests according to the change of `actix-rt`. `Arbiter::spawn` and `actix_rt::spawn` now panic outside the context of `actix::System`. They must be called inside `System::run`, `SystemRunner::run` or `SystemRunner::block_on`. More information can be found [here](IC717769654). [#447]
- `actix::fut::Either`'s internal variants' representation has changed to struct fields. [#453]
- Replace `pin_project` with `pin_project_lite` [#453]
- Update `crossbeam-channel` to `0.5`
- Update `bytes` to `1`. [#443]
- Update `tokio` to `1`. [#443]
- Update `tokio-util` tp `0.6`. [#443]

### Fixed

- Unified MessageResponse impl (combine separate Item/Error type, migrate to Item=Result). [#446]
- Fix error for build with `--no-default-features` flag, add `sink` feature for futures-util dependency. [#427]

### Removed

- Remove unnecessary `actix::clock::Duration` re-export of `std::time::Duration`. [#443]

[#421]: https://github.com/actix/actix/pull/421
[#424]: https://github.com/actix/actix/pull/424
[#427]: https://github.com/actix/actix/pull/427
[#432]: https://github.com/actix/actix/pull/432
[#435]: https://github.com/actix/actix/pull/435
[#439]: https://github.com/actix/actix/pull/439
[#443]: https://github.com/actix/actix/pull/443
[#445]: https://github.com/actix/actix/pull/445
[#446]: https://github.com/actix/actix/pull/446
[#447]: https://github.com/actix/actix/pull/447
[#448]: https://github.com/actix/actix/pull/448
[#453]: https://github.com/actix/actix/pull/453
[#IC717769654]: https://github.com/actix/actix-net/issues/206#issuecomment-717769654

## 0.10.0

### Changed

- `SinkWrite::write` calls now send all items correctly using an internal buffer. [#384]
- Add `Sync` bound for `Box<dyn Sender>` trait object that making `Recipient` a `Send` + `Sync` type. [#403]
- Update `parking_lot` to 0.11 [#404]
- Remove unnecessary `PhantomData` field from `Request` making it `Send + Sync` regardless if `Request`'s type-argument is `Send` or `Sync` [#407]

[#384]: https://github.com/actix/actix/pull/384
[#403]: https://github.com/actix/actix/pull/403
[#404]: https://github.com/actix/actix/pull/404
[#407]: https://github.com/actix/actix/pull/407

## 0.10.0-alpha.3

### Changed

- Update `tokio-util` dependency to 0.3, `FramedWrite` trait bound is changed. [#365]
- Only poll dropped ContextFut if event loop is running. [#374]
- Minimum Rust version is now 1.40 (to be able to use `#[cfg(doctest)]`)

[#365]: https://github.com/actix/actix/pull/365
[#374]: https://github.com/actix/actix/pull/374

### Fixed

- Fix `ActorFuture::poll_next` impl for `StreamThen` to not lose inner future when it's pending. [#376]

[#376]: https://github.com/actix/actix/pull/376

## 0.10.0-alpha.2

### Added

- New `AtomicResponse`, a `MessageResponse` with exclusive poll over actor's reference. [#357]

### Changed

- Require `Pin` for `ResponseActFuture`. [#355]

[#355]: https://github.com/actix/actix/pull/355
[#357]: https://github.com/actix/actix/pull/357

## 0.10.0-alpha.1

### Fixed

- Fix `MessageResponse` implementation for `ResponseFuture` to always poll the spawned `Future`. [#317]

### Added

- New method address on SyncContext [#341]
- Allow return of any `T: 'static` on `ResponseActFuture`. [#310]
- Allow return of any `T: 'static` on `ResponseFuture`. [#343]

### Changed

- Feature `http` was removed. Actix support for http was moved solely to actix-http and actix-web crates. [#324]
- Make `Pin`s safe [#335] [#346] [#347]
- Only implement `ActorFuture` for `Box` where `ActorFuture` is `Unpin` [#348]
- Upgrade `trust-dns-proto` to 0.19 [#349]
- Upgrade `trust-dns-resolver` to 0.19 [#349]

[#310]: https://github.com/actix/actix/pull/310
[#317]: https://github.com/actix/actix/pull/317
[#324]: https://github.com/actix/actix/pull/324
[#335]: https://github.com/actix/actix/pull/335
[#343]: https://github.com/actix/actix/pull/343
[#346]: https://github.com/actix/actix/pull/346
[#347]: https://github.com/actix/actix/pull/347
[#348]: https://github.com/actix/actix/pull/348
[#349]: https://github.com/actix/actix/pull/349

## 0.9.0

### Fixed

- Fix `ResolveFuture` type signature.

## 0.9.0-alpha.2

### Fixed

- Fix `Resolve` actor's panic

## 0.9.0-alpha.1

### Added

- Added `Context::connected()` to check any addresses are alive
- Added `fut::ready()` future

### Changed

- Migrate to std::future, tokio 0.2 and actix-rt 1.0.0 @bcmcmill #300
- Upgrade `derive_more` to 0.99.2
- Upgrade `smallvec` to 1.0.0

### Fixed

- Added `#[must_use]` attribute to `ActorFuture` and `ActorStream`

## 0.8.3

### Fixed

- Stop actor on async context drop

## 0.8.2

### Changed

- Enable `http` feature by default

## 0.8.1

### Added

- Added `std::error::Error` impl for `SendError`

### Fixed

- Fixed concurrent system registry insert #248

## 0.8.0

### Added

- Added `std::error::Error` impl for `MailboxError`

### Changed

- Use trust-dns-resolver 0.11.0

## 0.8.0.alpha.3

- Add `Actor::start_in_arbiter` with semantics of `Supervisor::start_in_arbiter`.
- Add `ResponseError` for `ResolverError`
- Add `io::SinkWrite`

## 0.8.0-alpha.2

- Add `actix-http` error support for MailboxError

## 0.8.0-alpha.1

### Changes

- Edition 2018
- Replace System/Arbiter with `actix_rt::System` and `actix_rt::Arbiter`
- Add implementations for `Message` for `Arc` and `Box`
- System and arbiter registries available via `from_registry()` method.

### Deleted

- Deleted signals actor. The functionality formerly provided by the signals actor can be achieved using the `tokio-signal` crate. See the [chat example] for how this can work.

[chat example]: ./examples/chat/src/main.rs

## 0.7.10

- Introduced methods `Sender::connected()`, `AddressSender::connected()` and `Recipient::connected()` to check availability of alive actor.
- Added `WeakAddr<A>` to weak reference an actor

## 0.7.9

- Removed `actix` module from prelude. See rationale in #161

## 0.7.8

- Update `crossbeam-channel` to 0.3 and `parking_lot` to 0.7

## 0.7.7

- Impl `Into<Recipient<M>>` for `Addr<A>`

## 0.7.6

- Use `trust-dns-resolver` 0.10.0.
- Make `System::stop_with_code` public.

## 0.7.5

### Added

- Introduce the `clock` module to allow overriding and mocking the system clock based on `tokio_timer`.
- System now has `System::builder()` which allows overriding the system clock with a custom instance. `Arbiter::builder()` can now also override the system clock. The default is to inherit from the system.
- New utility classes `TimerFunc` and `IntervalFunc` in the `utils` module.
- Implement `failure::Fail` for `SendError`.
- Implement `Debug` for multiple public types: `AddressSender`, `Addr`, `Arbiter`, `Context`, `ContextParts`, `ContextFut`, `Response`, `ActorResponse`, `Mailbox`, `SystemRegistry`, `Supervisor`, `System`, `SystemRunner`, `SystemArbiter`. #135

### Changed

- No longer perform unnecessary clone of `Addr` in `SystemRegistry::set`.
- Set min trust-dns version

### Fixed

- fix infinite loop in ContextFut::poll() caused by late cancel_future() #147

## 0.7.4

- Introduce method `query` to determine whether there is running actor in registry.
- Return back `mocker` module.

## 0.7.3

- Parked messages not getting processed #120

## 0.7.2

- Use actix-derive 0.3

## 0.7.1

- Arbiter now has `Arbiter::builder()` which allows opt-in of behavior to stop the actor system on uncaught panic in any arbiter thread. See #111 for examples.
- Allow to set custom system service actor via `SystemRegistry::set()` method.
- `AsyncContext::run_interval` does not fire callback immediately, instead it fires after specified duration.

## 0.7.0

- Context impl refactoring, fix potential UB
- Implemented `Eq`, `PartialEq`, and `Hash` for `actix::Addr`
- Implemented `Eq`, `PartialEq`, and `Hash` for `actix::Recipient`

## 0.6.2

- Breaking change: Restore `StreamHandler` from 0.5, new `StreamHandler` renamed to `StreamHandler2`

## 0.6.1

- Added `actix::run()` and `actix::spawn()` helper functions
- Use parking_lot 0.6
- Fixed potential memory unsafety

## 0.6.0

### Changed

- Use tokio
- `System` and `Arbiter` refactored
- `Arbiter::handle()` is not available anymore. Use `Arbiter::spawn()` and `Arbiter::spawn_fn()` instead.
- `StreamHandler` trait refactored.
- Min rustc version - 1.26

## 0.5.7

- Stop sync actor if sender is dead.

## 0.5.6

- Fix index usage during iteration for future cancellation #67

## 0.5.5

- Fix polling of wrong wait future after completion

## 0.5.4

- Always complete actor lifecycle (i.e. Actor::started())

## 0.5.3

- Panic after cancelling stream future #58

## 0.5.2

- Allow to set timeout for Connect message #56

## 0.5.1

- Internal state is alive during `stopping` period.
- Do not send StopArbiter message to system arbiter during system shutdown #53

## 0.5.0

- Address/Recipient is generic over actor destination
- Make rules of actor stopping more strict
- Use bounded channels for actor communications
- Add dns resolver and tcp connector utility actor
- Add `StreamHandler` trait for stream handling
- Add `Context::handle()` method, currently running future handle
- Add `actix::io` helper types for `AsyncWrite` related types
- Drop FramedContext

## 0.4.5

- Refactor context implementation
- Refactor Supervisor type
- Allow to use `Framed` instances with normal `Context`

## 0.4.4

- Add `Clone` implementation for `Box<Subscriber<M> + Send>`
- Stop stream polling if context is waiting for future completion
- Upgraded address stops working after all references are dropped #38

## 0.4.3

- Cleanup `FramedActor` error and close state handling.
- Do not exit early from framed polling

## 0.4.2

- Cleanup actor stopping process
- Unify context implementation

## 0.4.1

- Remove StreamHandler requirements from add_message_stream()
- Fix items length check

## 0.4.0

- Simplify `Handler` trait (E type removed).
- Use associated type for handler response for `Handler` trait.
- Added framed `drain` method.
- Allow to replace framed object in framed context.
- Enable signal actor by default, make it compatible with windows.
- Added `SyncContext::restart()` method, which allow to restart sync actor.
- Changed behaviour of `Address::call`, if request get drop message cancels.

## 0.3.5

- Re-export `actix_derive` package
- Added conversion implementation `From<Result<I, E>> for Response<A, M>`
- Expose the Framed underneath FramedContext #29

## 0.3.4

- Fix memory leak when sending messages recursively to self #28
- Add convenience impl for boxed Subscriber objects. #27
- Add `ActorStream::fold()` method.
- Add helper combinator `Stream::finish()` method.

## 0.3.3

- SystemRegistry does not store created actor #21

## 0.3.2

- Disable `signal` feature by default

## 0.3.1

- Simplify `ToEnvelope` trait, do not generalize over Message type.
- `ActorContext` requires `ToEnvelope` trait.
- Added `Subscriber::subscriber() -> Box<Subscriber>`
- Simplify `ActorContext` trait, it does not need to know about `Actor`
- Cancel `notify` and `run_later` futures on context stop

## 0.3.0

- Added `Either` future
- Message has to provide `ResponseType` impl instead of Actor

## 0.2.0

- Added `ActorStream`

## 0.1.0 (2017-10-11)

- First release
