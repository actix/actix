# actix-derive

> Derive macros for `actix` actors.

[![crates.io](https://img.shields.io/crates/v/actix-derive?label=latest)](https://crates.io/crates/actix-derive)
[![Documentation](https://docs.rs/actix-derive/badge.svg?version=0.6.0)](https://docs.rs/actix-derive/0.6.0)
[![Version](https://img.shields.io/badge/rustc-1.46+-ab6000.svg)](https://blog.rust-lang.org/2019/12/19/Rust-1.46.0.html)
![License](https://img.shields.io/crates/l/actix-derive.svg)
[![Dependency Status](https://deps.rs/crate/actix/0.6.0/status.svg)](https://deps.rs/crate/actix/0.6.0)

## Documentation

- [API Documentation](https://docs.rs/actix-derive)
- [API Documentation (master branch)](https://actix.rs/actix/actix_derive)

## Usage

```rust
use actix_derive::{Message, MessageResponse};

#[derive(MessageResponse)]
struct Added(usize);

#[derive(Message)]
#[rtype(result = "Added")]
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
#[rtype(result = "Added")]
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

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  https://opensource.org/licenses/MIT)

at your option.
