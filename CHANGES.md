# CHANGES

## [unreleased]

### Changed

* BREAKING: `SinkWrite::write` calls now send all items correctly using an internal buffer. [#384]

* Add `Sync` bound for `Box<dyn Sender>` trait object that making `Recipient` a `Send` + `Sync` type. [#403]

* Update `parking_lot` to 0.11 [#404]

* Remove unnecessary `PhantomData` field from `Request` making it `Send + Sync`
  regardless if `Request`'s type-argument is `Send` or `Sync` [#407]

[#384]: https://github.com/actix/actix/pull/384
[#403]: https://github.com/actix/actix/pull/403
[#404]: https://github.com/actix/actix/pull/404
[#407]: https://github.com/actix/actix/pull/407

## [0.10.0-alpha.3] - 2020-05-13

### Changed

* Update `tokio-util` dependency to 0.3, `FramedWrite` trait bound is changed. [#365]

* Only poll dropped ContextFut if event loop is running. [#374]

* Minimum Rust version is now 1.40 (to be able to use `#[cfg(doctest)]`)

[#365]: https://github.com/actix/actix/pull/365
[#374]: https://github.com/actix/actix/pull/374

### Fixed

* Fix `ActorFuture::poll_next` impl for `StreamThen` to not lose inner future when it's pending. [#376]

[#376]: https://github.com/actix/actix/pull/376

## [0.10.0-alpha.2] 2020-03-05

### Added

* New `AtomicResponse`, a `MessageResponse` with exclusive poll over actor's reference. [#357]

### Changed

* Require `Pin` for `ResponseActFuture`. [#355]

[#355]: https://github.com/actix/actix/pull/355
[#357]: https://github.com/actix/actix/pull/357

## [0.10.0-alpha.1] 2020-02-25

### Fixed

* Fix `MessageResponse` implementation  for `ResponseFuture` to always poll the spawned `Future`. [#317]

### Added

* New method address on SyncContext [#341]

* Allow return of any `T: 'static` on `ResponseActFuture`. [#310]

* Allow return of any `T: 'static` on `ResponseFuture`. [#343]

### Changed

* Feature `http` was removed. Actix support for http was moved solely to actix-http and actix-web crates. [#324]

* Make `Pin`s safe [#335] [#346] [#347]

* Only implement `ActorFuture` for `Box` where `ActorFuture` is `Unpin` [#348]

* Upgrade `trust-dns-proto` to 0.19 [#349]

* Upgrade `trust-dns-resolver` to 0.19 [#349]

[#310]: https://github.com/actix/actix/pull/310
[#317]: https://github.com/actix/actix/pull/317
[#324]: https://github.com/actix/actix/pull/324
[#335]: https://github.com/actix/actix/pull/335
[#343]: https://github.com/actix/actix/pull/343
[#346]: https://github.com/actix/actix/pull/346
[#347]: https://github.com/actix/actix/pull/347
[#348]: https://github.com/actix/actix/pull/348
[#349]: https://github.com/actix/actix/pull/349

## [0.9.0] 2019-12-20

### Fixed

* Fix `ResolveFuture` type signature.

## [0.9.0-alpha.2] 2019-12-16

### Fixed

* Fix `Resolve` actor's panic

## [0.9.0-alpha.1] 2019-12-15

### Added

* Added `Context::connected()` to check any addresses are alive

* Added `fut::ready()` future

### Changed

* Migrate to std::future, tokio 0.2 and actix-rt 1.0.0 @bcmcmill #300

* Upgrade `derive_more` to 0.99.2

* Upgrade `smallvec` to 1.0.0

### Fixed

* Added `#[must_use]` attribute to `ActorFuture` and `ActorStream`


## [0.8.3] (2019-05-29)

### Fixed

* Stop actor on async context drop


## [0.8.2] (2019-05-12)

### Changed

* Enable `http` feature by default


## [0.8.1] (2019-04-16)

### Added

* Added `std::error::Error` impl for `SendError`

### Fixed

* Fixed concurrent system registry insert #248


## [0.8.0] (2019-04-14)

### Added

* Added `std::error::Error` impl for `MailboxError`

### Changed

* Use trust-dns-resolver 0.11.0


## [0.8.0.alpha.3] (2019-04-12)

### Added

* Add `Actor::start_in_arbiter` with semantics of `Supervisor::start_in_arbiter`.

* Add `ResponseError` for `ResolverError`

* Add `io::SinkWrite`


## [0.8.0-alpha.2] (2019-03-29)

### Added

* Add `actix-http` error support for MailboxError


## [0.8.0-alpha.1] (2019-03-28)

### Changes

* Edition 2018

* Replace System/Arbiter with `actix_rt::System` and `actix_rt::Arbiter`

* Add implementations for `Message` for `Arc` and `Box`

* System and arbiter registries available via `from_registry()` method.

### Deleted

* Deleted signals actor. The functionality formerly provided by the
  signals actor can be achieved using the `tokio-signal` crate. See
  the [chat example] for how this can work.

  [chat example]: ./examples/chat/src/main.rs

## [0.7.10] (2019-01-16)

### Changed

- Introduced methods `Sender::connected()`, `AddressSender::connected()`
  and `Recipient::connected()` to check availability of alive actor.

- Added `WeakAddr<A>` to weak reference an actor

## [0.7.9] (2018-12-11)

### Changed

- Removed `actix` module from prelude. See rationale in #161

## [0.7.8] (2018-12-05)

### Changed

- Update `crossbeam-channel` to 0.3 and `parking_lot` to 0.7

## [0.7.7] (2018-11-22)

### Added

- Impl `Into<Recipient<M>>` for `Addr<A>`

## [0.7.6] (2018-11-08)

### Changed

- Use `trust-dns-resolver` 0.10.0.

- Make `System::stop_with_code` public.

## [0.7.5] (2018-10-10)

### Added

- Introduce the `clock` module to allow overriding and mocking the system clock
  based on `tokio_timer`.

- System now has `System::builder()` which allows overriding the system clock
  with a custom instance. `Arbiter::builder()` can now also override the system
  clock. The default is to inherit from the system.

- New utility classes `TimerFunc` and `IntervalFunc` in the `utils` module.

- Implement `failure::Fail` for `SendError`.

- Implement `Debug` for multiple public types: `AddressSender`, `Addr`, `Arbiter`, `Context`, `ContextParts`, `ContextFut`, `Response`, `ActorResponse`, `Mailbox`, `SystemRegistry`, `Supervisor`, `System`, `SystemRunner`, `SystemArbiter`. #135


### Changed

- No longer perform unnecessary clone of `Addr` in `SystemRegistry::set`.

- Set min trust-dns version


### Fixed

- fix infinite loop in ContextFut::poll() caused by late cancel_future() #147


## [0.7.4] (2018-08-27)

### Added

* Introduce method `query` to determine whether there is running actor in registry.

* Return back `mocker` module.


## [0.7.3] (2018-07-30)

### Fixed

* Parked messages not getting processed #120


## [0.7.2] (2018-07-24)

### Changed

* Use actix-derive 0.3


## [0.7.1] (2018-07-20)

### Added

* Arbiter now has `Arbiter::builder()` which allows opt-in of behavior to stop
  the actor system on uncaught panic in any arbiter thread. See #111 for examples.

* Allow to set custom system service actor via `SystemRegistry::set()` method.

### Fixed

* `AsyncContext::run_interval` does not fire callback immediately, instead it fires after specified duration.


## [0.7.0] (2018-07-05)

### Changed

* Context impl refactoring, fix potential UB

### Added

* Implemented `Eq`, `PartialEq`, and `Hash` for `actix::Addr`

* Implemented `Eq`, `PartialEq`, and `Hash` for `actix::Recipient`


## [0.6.2] (2018-06-xx)

### Changed

* Breaking change: Restore `StreamHandler` from 0.5, new `StreamHandler` renamed to `StreamHandler2`


## [0.6.1] (2018-06-19)

### Added

* Added `actix::run()` and `actix::spawn()` helper functions


### Changed

* Use parking_lot 0.6


### Fixed

* Fixed potential memory unsafety


## [0.6.0] (2018-06-18)

### Changed

* Use tokio

* `System` and `Arbiter` refactored

* `Arbiter::handle()` is not available anymore.
  Use `Arbiter::spawn()` and `Arbiter::spawn_fn()` instead.

* `StreamHandler` trait refactored.

* Min rustc version - 1.26


## 0.5.7 (2018-05-17)

* Stop sync actor if sender is dead.


## 0.5.6 (2018-04-17)

* Fix index usage during iteration for future cancellation #67


## 0.5.5 (2018-03-19)

* Fix polling of wrong wait future after completion


## 0.5.4 (2018-03-16)

* Always complete actor lifecycle (i.e. Actor::started())


## 0.5.3 (2018-03-08)

* Panic after cancelling stream future #58


## 0.5.2 (2018-03-06)

* Allow to set timeout for Connect message #56


## 0.5.1 (2018-03-02)

* Internal state is alive during `stopping` period.

* Do not send StopArbiter message to system arbiter during system shutdown #53


## 0.5.0 (2018-02-17)

* Address/Recipient is generic over actor destination

* Make rules of actor stopping more strict

* Use bounded channels for actor communications

* Add dns resolver and tcp connector utility actor

* Add `StreamHandler` trait for stream handling

* Add `Context::handle()` method, currently running future handle

* Add `actix::io` helper types for `AsyncWrite` related types

* Drop FramedContext


## 0.4.5 (2018-01-23)

* Refactor context implementation

* Refactor Supervisor type

* Allow to use `Framed` instances with normal `Context`


## 0.4.4 (2018-01-19)

* Add `Clone` implementation for `Box<Subscriber<M> + Send>`

* Stop stream polling if context is waiting for future completion

* Upgraded address stops working after all references are dropped #38


## 0.4.3 (2018-01-09)

* Cleanup `FramedActor` error and close state handling.

* Do not exit early from framed polling


## 0.4.2 (2018-01-07)

* Cleanup actor stopping process

* Unify context implementation


## 0.4.1 (2018-01-06)

* Remove StreamHandler requirements from add_message_stream()

* Fix items length check


## 0.4.0 (2018-01-05)

* Simplify `Handler` trait (E type removed).

* Use associated type for handler response for `Handler` trait.

* Added framed `drain` method.

* Allow to replace framed object in framed context.

* Enable signal actor by default, make it compatible with windows.

* Added `SyncContext::restart()` method, which allow to restart sync actor.

* Changed behaviour of `Address::call`, if request get drop message cancels.


## 0.3.5 (2017-12-23)

* Re-export `actix_derive` package

* Added conversion implementation `From<Result<I, E>> for Response<A, M>`

* Expose the Framed underneath FramedContext #29


## 0.3.4 (2017-12-20)

* Fix memory leak when sending messages recursively to self #28

* Add convenience impl for boxed Subscriber objects. #27

* Add `ActorStream::fold()` method.

* Add helper combinator `Stream::finish()` method.


## 0.3.3 (2017-11-21)

* SystemRegistry does not store created actor #21


## 0.3.2 (2017-11-06)

* Disable `signal` feature by default


## 0.3.1 (2017-10-30)

* Simplify `ToEnvelope` trait, do not generalize over Message type.

* `ActorContext` requires `ToEnvelope` trait.

* Added `Subscriber::subscriber() -> Box<Subscriber>`

* Simplify `ActorContext` trait, it does not need to know about `Actor`

* Cancel `notify` and `run_later` futures on context stop


## 0.3.0 (2017-10-23)

* Added `Either` future

* Message has to provide `ResponseType` impl instead of Actor


## 0.2.0 (2017-10-17)

* Added `ActorStream`


## 0.1.0 (2017-10-11)

* First release
