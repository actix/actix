<div align="center">
  <h1>Actix</h1>
  <p>
    <strong>Actor framework for Rust</strong>
  </p>
  <p>

[![crates.io](https://meritbadge.herokuapp.com/actix)](https://crates.io/crates/actix)
[![Documentation](https://docs.rs/actix/badge.svg)](https://docs.rs/actix)
[![Version](https://img.shields.io/badge/rustc-1.46+-ab6000.svg)](https://blog.rust-lang.org/2019/12/19/Rust-1.46.0.html)
<br />
![License](https://img.shields.io/crates/l/actix.svg)
[![codecov](https://codecov.io/gh/actix/actix/branch/master/graph/badge.svg)](https://codecov.io/gh/actix/actix)
[![Download](https://img.shields.io/crates/d/actix.svg)](https://crates.io/crates/actix)
[![Join the chat at https://gitter.im/actix/actix](https://badges.gitter.im/actix/actix.svg)](https://gitter.im/actix/actix?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

  </p>
</div>

- [User Guide](https://actix.rs/book/actix/)
- [API Documentation](https://docs.rs/actix/)
- [API Documentation (master branch)](https://actix.github.io/actix/actix/)

| Platform | Build Status                                                                                                                                              |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linux    | [![build status](https://github.com/actix/actix/workflows/CI%20%28Linux%29/badge.svg?branch=master&event=push)](https://github.com/actix/actix/actions)   |
| macOS    | [![build status](https://github.com/actix/actix/workflows/CI%20%28macOS%29/badge.svg?branch=master&event=push)](https://github.com/actix/actix/actions)   |
| Windows  | [![build status](https://github.com/actix/actix/workflows/CI%20%28Windows%29/badge.svg?branch=master&event=push)](https://github.com/actix/actix/actions) |

---

## Features

- Async and sync actors
- Actor communication in a local/thread context
- Uses [futures](https://crates.io/crates/futures) for asynchronous message handling
- Actor supervision
- Typed messages (No `Any` type)
- Runs on stable Rust 1.46+

## Usage

To use `actix`, add this to your `Cargo.toml`:

```toml
[dependencies]
actix = "0.10"
```

### Initialize Actix

In order to use actix you first need to create a `System`.

```rust,ignore
fn main() {
    let system = actix::System::new();

    system.run();
}
```

Actix uses the [tokio](https://github.com/tokio-rs/tokio) event loop.
`System::new()` creates a new event loop and starts the `System` actor.
`system.run()` starts the tokio event loop, and will finish once the `System` actor
receives the `SystemExit` message.

Let's create a simple Actor.

### Implement an Actor

In order to define an actor you need to define a struct and have it implement
the [`Actor`](https://actix.github.io/actix/actix/trait.Actor.html) trait.

```rust
use actix::{Actor, Addr, Context, System};

struct MyActor;

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("I am alive!");
        System::current().stop(); // <- stop system
    }
}

fn main() {
    let mut system = System::new();

    let addr = system.block_on(async { MyActor.start() });

    system.run();
}
```

Spawning a new actor is achieved via the `start` and `create` methods of
the [Actor](https://actix.github.io/actix/actix/trait.Actor.html)
trait. It provides several different ways of creating actors; for details, check the docs.
You can implement the `started`, `stopping` and `stopped` methods of the Actor trait.
`started` gets called when the actor starts and `stopping` when the actor finishes.
Check the [API documentation](https://actix.github.io/actix/actix/trait.Actor.html#actor-lifecycle)
for more information on the actor lifecycle.

### Handle Messages

An Actor communicates with another Actor by sending messages. In actix all messages
are typed. Let's define a simple `Sum` message with two `usize` parameters
and an actor which will accept this message and return the sum of those two numbers.
Here we use the `#[actix::main]` attribute as a way to start our `System`
and drive our main `Future` so we can easily `.await` for the messages sent to the `Actor`.

```rust
use actix::prelude::*;

// this is our Message
#[derive(Message)]
#[rtype(result = "usize")] // we have to define the response type for `Sum` message
struct Sum(usize, usize);

// Actor definition
struct Summator;

impl Actor for Summator {
    type Context = Context<Self>;
}

// now we need to define `MessageHandler` for the `Sum` message.
impl Handler<Sum> for Summator {
    type Result = usize; // <- Message response type

    fn handle(&mut self, msg: Sum, ctx: &mut Context<Self>) -> Self::Result {
        msg.0 + msg.1
    }
}

#[actix::main] // <- starts the system and block until future resolves
async fn main() {
    let addr = Summator.start();
    let res = addr.send(Sum(10, 5)).await; // <- send message and get future for result

    match res {
        Ok(result) => println!("SUM: {}", result),
        _ => println!("Communication to the actor has failed"),
    }
}
```

All communications with actors go through an `Addr` object. You can `do_send` a message
without waiting for a response, or you can `send` an actor a specific message. The `Message`
trait defines the result type for a message.

### Actor State And Subscription For Specific Messages

You may have noticed that the methods of the `Actor` and `Handler` traits accept `&mut self`, so you are
welcome to store anything in an actor and mutate it whenever necessary.

Address objects require an actor type, but if we just want to send a specific message to
an actor that can handle the message, we can use the `Recipient` interface. Let's create
a new actor that uses `Recipient`.

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
    addr: Recipient<Ping>,
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
                act.addr.do_send(Ping { id: msg.id + 1 });
            });
        }
    }
}

fn main() {
    let mut system = System::new();

    // To get a Recipient object, we need to use a different builder method
    // which will allow postponing actor creation
    let addr = system.block_on(async {
        Game::create(|ctx| {
                // now we can get an address of the first actor and create the second actor
                let addr = ctx.address();
                let addr2 = Game {
                    counter: 0,
                    name: String::from("Game 2"),
                    addr: addr.recipient(),
                }
                .start();

                // let's start pings
                addr2.do_send(Ping { id: 10 });

                // now we can finally create first actor
                Game {
                    counter: 0,
                    name: String::from("Game 1"),
                    addr: addr2.recipient(),
                }
            });
    });

    system.run();
}
```

### Chat Example

There is a
[chat example](https://github.com/actix/actix/tree/master/examples/chat)
which provides a basic example of networking client/server service.

## Contributing

All contributions are welcome, if you have a feature request don't hesitate to open an issue!

## License

This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  https://opensource.org/licenses/MIT)

at your option.

## Code of Conduct

Contribution to the actix crate is organized under the terms of the
Contributor Covenant, the maintainers of actix, promises to
intervene to uphold that code of conduct.
