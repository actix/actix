<div align="center">
  <h1>Actix</h1>
  <p>
    <strong>Actor framework for Rust</strong>
  </p>
  <p>

<!-- prettier-ignore-start -->

[![crates.io](https://img.shields.io/crates/v/actix?label=latest)](https://crates.io/crates/actix)
[![Documentation](https://docs.rs/actix/badge.svg?version=0.13.5)](https://docs.rs/actix/0.13.5)
![Minimum Supported Rust Version](https://img.shields.io/badge/rustc-1.68+-ab6000.svg)
![License](https://img.shields.io/crates/l/actix.svg)
[![Dependency Status](https://deps.rs/crate/actix/0.13.5/status.svg)](https://deps.rs/crate/actix/0.13.5)
<br />
[![CI](https://github.com/actix/actix/actions/workflows/ci.yml/badge.svg)](https://github.com/actix/actix/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/actix/actix/branch/master/graph/badge.svg)](https://codecov.io/gh/actix/actix)
![Downloads](https://img.shields.io/crates/d/actix.svg)
[![Chat on Discord](https://img.shields.io/discord/771444961383153695?label=chat&logo=discord)](https://discord.gg/GMuKN5b8aR)

<!-- prettier-ignore-end -->

  </p>
</div>

## Documentation

- [User Guide](https://actix.rs/docs/actix)
- [API Documentation](https://docs.rs/actix)

## Features

- Async and sync actors
- Actor communication in a local/thread context
- Uses [futures](https://crates.io/crates/futures) for asynchronous message handling
- Actor supervision
- Typed messages (No `Any` type)
- Runs on stable Rust 1.68+

## Usage

To use `actix`, add this to your `Cargo.toml`:

```toml
[dependencies]
actix = "0.13"
```

### Initialize Actix

In order to use actix you first need to create a `System`.

```rust,ignore
fn main() {
    let system = actix::System::new();

    system.run();
}
```

Actix uses the [Tokio](https://github.com/tokio-rs/tokio) runtime. `System::new()` creates a new event loop. `System.run()` starts the Tokio event loop, and will finish once the `System` actor receives the `SystemExit` message.

### Implementing an Actor

In order to define an actor you need to define a struct and have it implement the [`Actor`](https://docs.rs/actix/latest/actix/trait.Actor.html) trait.

```rust
use actix::{Actor, Context, System};

struct MyActor;

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        println!("I am alive!");
        System::current().stop(); // <- stop system
    }
}

fn main() {
    let system = System::new();

    let _addr = system.block_on(async { MyActor.start() });

    system.run().unwrap();
}
```

Spawning a new actor is achieved via the `start` and `create` methods of the [Actor trait]. It provides several different ways of creating actors; for details, check the docs. You can implement the `started`, `stopping` and `stopped` methods of the Actor trait. `started` gets called when the actor starts and `stopping` when the actor finishes. Check the API docs for more information on [the actor lifecycle].

[Actor trait]: https://docs.rs/actix/latest/actix/trait.Actor.html
[the actor lifecycle]: https://actix.rs/docs/actix/actor#actor-lifecycle

### Handle Messages

An Actor communicates with another Actor by sending messages. In actix all messages are typed. Let's define a simple `Sum` message with two `usize` parameters and an actor which will accept this message and return the sum of those two numbers. Here we use the `#[actix::main]` attribute as an easier way to start our `System` and drive our main function so we can easily `.await` for the responses sent back from the `Actor`.

```rust
use actix::prelude::*;

// this is our Message
// we have to define the response type (rtype)
#[derive(Message)]
#[rtype(usize)]
struct Sum(usize, usize);

// Actor definition
struct Calculator;

impl Actor for Calculator {
    type Context = Context<Self>;
}

// now we need to implement `Handler` on `Calculator` for the `Sum` message.
impl Handler<Sum> for Calculator {
    type Result = usize; // <- Message response type

    fn handle(&mut self, msg: Sum, _ctx: &mut Context<Self>) -> Self::Result {
        msg.0 + msg.1
    }
}

#[actix::main] // <- starts the system and block until future resolves
async fn main() {
    let addr = Calculator.start();
    let res = addr.send(Sum(10, 5)).await; // <- send message and get future for result

    match res {
        Ok(result) => println!("SUM: {}", result),
        _ => println!("Communication to the actor has failed"),
    }
}
```

All communications with actors go through an `Addr` object. You can `do_send` a message without waiting for a response, or you can `send` an actor a specific message. The `Message` trait defines the result type for a message.

### Actor State And Subscription For Specific Messages

You may have noticed that the methods of the `Actor` and `Handler` traits accept `&mut self`, so you are welcome to store anything in an actor and mutate it whenever necessary.

Address objects require an actor type, but if we just want to send a specific message to an actor that can handle the message, we can use the `Recipient` interface. Let's create a new actor that uses `Recipient`.

```rust
use actix::prelude::*;
use std::time::Duration;

#[derive(Message)]
#[rtype(result = "()")]
struct Ping {
    pub id: usize,
}

// Actor definition
struct Game {
    counter: usize,
    name: String,
    recipient: Recipient<Ping>,
}

impl Actor for Game {
    type Context = Context<Game>;
}

// simple message handler for Ping message
impl Handler<Ping> for Game {
    type Result = ();

    fn handle(&mut self, msg: Ping, ctx: &mut Context<Self>) {
        self.counter += 1;

        if self.counter > 10 {
            System::current().stop();
        } else {
            println!("[{0}] Ping received {1}", self.name, msg.id);

            // wait 100 nanoseconds
            ctx.run_later(Duration::new(0, 100), move |act, _| {
                act.recipient.do_send(Ping { id: msg.id + 1 });
            });
        }
    }
}

fn main() {
    let system = System::new();

    system.block_on(async {
        // To create a cyclic game link, we need to use a different constructor
        // method to get access to its recipient before it starts.
        let _game = Game::create(|ctx| {
            // now we can get an address of the first actor and create the second actor
            let addr = ctx.address();

            let addr2 = Game {
                counter: 0,
                name: String::from("Game 2"),
                recipient: addr.recipient(),
            }
            .start();

            // let's start pings
            addr2.do_send(Ping { id: 10 });

            // now we can finally create first actor
            Game {
                counter: 0,
                name: String::from("Game 1"),
                recipient: addr2.recipient(),
            }
        });
    });

    // let the actors all run until they've shut themselves down
    system.run().unwrap();
}
```

### Chat Example

See this [chat example] which shows more comprehensive usage in a networking client/server service.

[chat example]: https://github.com/actix/examples/tree/HEAD/websockets/chat-tcp

## Contributing

All contributions are welcome, if you have a feature request don't hesitate to open an issue!

## License

This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

## Code of Conduct

Contribution to the actix repo is organized under the terms of the Contributor Covenant. The Actix team promises to intervene to uphold that code of conduct.
