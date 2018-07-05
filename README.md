# Actix [![Build Status](https://travis-ci.org/actix/actix.svg?branch=master)](https://travis-ci.org/actix/actix) [![Build status](https://ci.appveyor.com/api/projects/status/aytxo1w6a88x2cxk/branch/master?svg=true)](https://ci.appveyor.com/project/fafhrd91/actix-n9e64/branch/master) [![codecov](https://codecov.io/gh/actix/actix/branch/master/graph/badge.svg)](https://codecov.io/gh/actix/actix) [![crates.io](http://meritbadge.herokuapp.com/actix)](https://crates.io/crates/actix) [![Join the chat at https://gitter.im/actix/actix](https://badges.gitter.im/actix/actix.svg)](https://gitter.im/actix/actix?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

Actix is a Rust actors framework.

* [User Guide](https://actix.rs/book/actix/)
* [API Documentation (Development)](http://actix.github.io/actix/actix/)
* [API Documentation (Releases)](https://docs.rs/actix/)
* Cargo package: [actix](https://crates.io/crates/actix)
* Minimum supported Rust version: 1.26 or later

---

## Features

  * Async/Sync actors.
  * Actor communication in a local/thread context.
  * Uses [Futures](https://crates.io/crates/futures) for asynchronous message handling.
  * HTTP1/HTTP2 support ([actix-web](https://github.com/actix/actix-web))
  * Actor supervision.
  * Typed messages (No `Any` type).


## Usage

To use `actix`, add this to your `Cargo.toml`:

```toml
[dependencies]
actix = "0.7"
```

### Initialize Actix

In order to use actix you first need to create a `System`.

```rust,ignore
extern crate actix;

fn main() {
    let system = actix::System::new("test");
    
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
extern crate actix;
use actix::{msgs, Actor, Addr, Arbiter, Context, Syn, System};

struct MyActor;

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
       println!("I am alive!");
       System::current().stop(); // <- stop system
    }
}

fn main() {
    let system = System::new("test");

    let addr = MyActor.start();

    system.run();
}
```

Spawning a new actor is achieved via the `start` and `create` methods of
the [Actor](https://actix.github.io/actix/actix/trait.Actor.html) 
trait. It provides several different ways of creating actors, for details check docs. 
You can implement `started`, `stopping` and `stopped` methods of the Actor trait. 
`started` gets called when actor starts and `stopping` when actor finishes.
Check [API documentation](https://actix.github.io/actix/actix/trait.Actor.html#actor-lifecycle) 
for more information on the actor lifecycle.

### Handle messages

An Actor communicates with another Actor by sending messages. In actix all messages
are typed. Let's define a simple `Sum` message with two `usize` parameters,
and an actor which will accept this message and return the sum of those two numbers.

```rust
extern crate actix;
extern crate futures;
use futures::{future, Future};
use actix::*;

// this is our Message
struct Sum(usize, usize);

// we have to define the response type for `Sum` message
impl Message for Sum {
    type Result = usize;
}

// Actor definition
struct Summator;

impl Actor for Summator {
    type Context = Context<Self>;
}

// now we need to define `MessageHandler` for the `Sum` message.
impl Handler<Sum> for Summator {
    type Result = usize;   // <- Message response type

    fn handle(&mut self, msg: Sum, ctx: &mut Context<Self>) -> Self::Result {
        msg.0 + msg.1
    }
}

fn main() {
    let sys = System::new("test");

    let addr = Summator.start();
    let res = addr.send(Sum(10, 5));  // <- send message and get future for result
    
    Arbiter::spawn(res.then(|res| {
        match res {
            Ok(result) => println!("SUM: {}", result),
            _ => println!("Something wrong"),
        }
        
        System::current().stop();
        future::result(Ok(()))
    }));

    sys.run();
}
```

All communications with actors go through an `Addr` object. You can `do_send` a message
without waiting for a response, or `send` an actor with specific message. The `Message`
trait defines the result type for a message.

### Actor state and subscription for specific messages

You may have noticed that methods of `Actor` and `Handler` traits accept `&mut self`, so you are
welcome to store anything in an actor and mutate it whenever necessary.

Address objects require an actor type, but if we just want to send a specific message to 
an actor that can handle the message, we can use the `Recipient` interface. Let's create
a new actor that uses `Recipient`.

```rust
#[macro_use] extern crate actix;
use std::time::Duration;
use actix::prelude::*;

#[derive(Message)]
struct Ping { pub id: usize }

// Actor definition
struct Game {
    counter: usize, 
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
            println!("Ping received {:?}", msg.id);
            
            // wait 100 nanos
            ctx.run_later(Duration::new(0, 100), move |act, _| {
                act.addr.do_send(Ping{id: msg.id + 1});
            });
        }
    }
}

fn main() {
    let system = System::new("test");

    // To get a Recipient object, we need to use a different builder method
    // which will allow postponing actor creation
    let addr = Game::create(|ctx| {
        // now we can get an address of the first actor and create the second actor
        let addr = ctx.address();
        let addr2 = Game{counter: 0, addr: addr.recipient()}.start();
        
        // let's start pings
        addr2.do_send(Ping{id: 10});
        
        // now we can finally create first actor
        Game{counter: 0, addr: addr2.recipient()}
    });

    system.run();
}
```

More information on signal handling is in the
[signal](https://actix.github.io/actix/actix/actors/signal/index.html) module.


### chat example

There is a
[chat example](https://github.com/actix/actix/tree/master/examples/chat) 
which provides a basic example of networking client/server service.


### fectl

You may consider checking out [fectl](https://github.com/fafhrd91/fectl) utility. It is written
with `actix` and shows how to create networking application with relatively complex interactions.

## Contributing

All contributions are welcome, if you have a feature request don't hesitate to open an issue!

## License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

## Code of Conduct

Contribution to the actix-web crate is organized under the terms of the
Contributor Covenant, the maintainer of actix, @fafhrd91, promises to
intervene to uphold that code of conduct.
