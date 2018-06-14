## 0.6

* `actix::System` has new api.

    Instead of
    
    ```rust
    fn main() {
        let sys = actix::System::new(..);
        
        Actor::start()
        
        sys.run();
    }
    ```

    Server must be initialized within system run closure:

    ```rust
    fn main() {
        actix::System::run(|| {
            Actor::start()
        });
    }
    ```

* `Addr` get refactored. `Syn` and `Unsync` get removed. all addresses are
  `Syn` now, only `Addr<Actor>` exsits

* `Arbiter` uses `tokio::runtime::current_thread`

* `Arbiter::handle()` get removed use `tokio::spawn()` instead or you can send
  `Execute` message to the `System`.

* `Arbiter::system_arbiter()` get removed. `System` does not run `Arbiter`.

* Actor can be started only within running tokio runtime or
  with `System::run()` or `SystemRuntime::config()`

* [`StreamHandler`](https://actix.rs/actix/actix/trait.StreamHandler.html) trait has been refactored. `StreamHandler::started()` is removed. `.error()` and `.finished()`
    methods get consolidated to the `.handle()` method.

* Use `actix::actors::resolver` instead of
  `actix::actors::resolver::{Connect, ConnectAddr, Connector, ConnectorError, Resolve};`
  
* `actix::actors::resolver::Connector` renamed to `actix::actors::resolver::Resolver`

* `actix::actors::resolver::ConnectorError` renamed to `actix::actors::resolver::ResolverError`
