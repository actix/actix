## 0.7.9

- `actix` no longer re-exports itself in `actix::prelude` to avoid conflicts with 2018 editions. Please access it through your `extern crate actix` import when necessary

## 0.7.6

- `trust-dns-resolver` dependency was bumped to version 0.10.0. If you use a
  custom resolver, you will need to switch your `ResolverConfig` and
  `ResolverOpts` to `trust-dns-resolver` 0.10.0 instead of 0.9.1.

## 0.7

* `Addr` get refactored. `Syn` and `Unsync` removed. all addresses are
  `Syn` now, only `Addr<Actor>` exsits

* `Arbiter` uses `tokio::runtime::current_thread`

* `Arbiter::arbiter()` renamed to `Arbiter::current()`

* `Arbiter::handle()` get removed use `Arbiter::spawn()` instead or you can send
  `Execute` message to the `System`.

* `Arbiter::system_arbiter()` get removed use `System::arbiter()` instead.

* Use `actix::actors::resolver` instead of
  `actix::actors::resolver::{Connect, ConnectAddr, Connector, ConnectorError, Resolve};`

* `actix::actors::resolver::Connector` renamed to `actix::actors::resolver::Resolver`

* `actix::actors::resolver::ConnectorError` renamed to `actix::actors::resolver::ResolverError`
