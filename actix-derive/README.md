# actix-derive

> Derive macros for `actix` actors.

<!-- prettier-ignore-start -->

[![crates.io](https://img.shields.io/crates/v/actix-derive?label=latest)](https://crates.io/crates/actix-derive)
[![Documentation](https://docs.rs/actix-derive/badge.svg?version=0.6.2)](https://docs.rs/actix-derive/0.6.2)
![Minimum Supported Rust Version](https://img.shields.io/badge/rustc-1.68+-ab6000.svg)
![License](https://img.shields.io/crates/l/actix-derive.svg)
[![Dependency Status](https://deps.rs/crate/actix/0.6.2/status.svg)](https://deps.rs/crate/actix/0.6.2)

<!-- prettier-ignore-end -->

## Usage

```rust
use actix_derive::{Message, MessageResponse};

#[derive(MessageResponse)]
struct Added(usize);

#[derive(Message)]
#[rtype(Added)]
struct Sum(usize, usize);

fn main() {}
```

This code expands into following code:

```rust
use actix::{Actor, Context, Handler, System};
use actix_derive::{Message, MessageResponse};

#[derive(MessageResponse)]
struct Added(usize);

#[derive(Message)]
#[rtype(Added)]
struct Sum(usize, usize);

#[derive(Default)]
struct Adder;

impl Actor for Adder {
    type Context = Context<Self>;
}

impl Handler<Sum> for Adder {
    type Result = <Sum as actix::Message>::Result;
    fn handle(&mut self, msg: Sum, _: &mut Self::Context) -> Added {
        Added(msg.0 + msg.1)
    }
}

fn main() {}
```

## License

This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.
