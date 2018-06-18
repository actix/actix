## 0.6

* `Addr` get refactored. `Syn` and `Unsync` removed. all addresses are
  `Syn` now, only `Addr<Actor>` exsits

* `Arbiter` uses `tokio::runtime::current_thread`

* `Arbiter::arbiter()` renamed to `Arbiter::current()`

* `Arbiter::handle()` get removed use `Arbiter::spawn()` instead or you can send
  `Execute` message to the `System`.

* `Arbiter::system_arbiter()` get removed use `System::arbiter()` instead.

* [`StreamHandler`](https://actix.rs/actix/actix/trait.StreamHandler.html) trait has been refactored. `StreamHandler::started()` is removed. `.error()` and `.finished()`
    methods get consolidated to the `.handle()` method.

* Use `actix::actors::resolver` instead of
  `actix::actors::resolver::{Connect, ConnectAddr, Connector, ConnectorError, Resolve};`
  
* `actix::actors::resolver::Connector` renamed to `actix::actors::resolver::Resolver`

* `actix::actors::resolver::ConnectorError` renamed to `actix::actors::resolver::ResolverError`
