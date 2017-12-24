# Actix [![Build Status](https://travis-ci.org/actix/actix.svg?branch=master)](https://travis-ci.org/actix/actix) [![Build status](https://ci.appveyor.com/api/projects/status/aytxo1w6a88x2cxk/branch/master?svg=true)](https://ci.appveyor.com/project/fafhrd91/actix-n9e64/branch/master) [![codecov](https://codecov.io/gh/actix/actix/branch/master/graph/badge.svg)](https://codecov.io/gh/actix/actix) [![crates.io](http://meritbadge.herokuapp.com/actix)](https://crates.io/crates/actix)

Actix is a rust actor framework.

* [API Documentation (Development)](http://actix.github.io/actix/actix/)
* [API Documentation (Releases)](https://docs.rs/actix/)
* Cargo package: [actix](https://crates.io/crates/actix)

---

## Features

  * Async/Sync actors.
  * Actor communication in a local/thread context.
  * Using Futures for asynchronous message handling.
  * HTTP1/HTTP2 support ([actix-web](https://github.com/actix/actix-web))
  * Actor supervision.
  * Typed messages (No `Any` type). Generic messages are allowed.


## Usage

To use `actix`, add this to your `Cargo.toml`:

```toml
[dependencies]
actix = "0.3"
```

You may consider to check 
[chat example](https://github.com/actix/actix/tree/master/examples/chat). 

### Initialize the Actix

In order to use actix you first need to create an `System`.

```rust,ignore
extern crate actix;

fn main() {
    let system = actix::System::new("test");
    
    system.run();
}
```

Actix uses [tokio](https://github.com/tokio-rs/tokio-core) event loop. 
`System::new()` call creates new event loop and starts `System` actor.
`system.run()` starts tokio event loop and will finish once the `System` actor 
receives `SystemExit` message.

Let's create simple Actor.

### Implement an Actor

In order to define an actor you need to define a struct and have it implement 
the [`Actor`](https://actix.github.io/actix/actix/trait.Actor.html) trait.


```rust
extern crate actix;
use actix::{msgs, Actor, Address, Arbiter, Context, System};

struct MyActor;

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
       println!("I am alive!");
       Arbiter::system().send(msgs::SystemExit(0));
    }
}

fn main() {
    let system = System::new("test");

    let addr: Address<_> = MyActor.start();

    system.run();
}
```

Spawning a new actor is achieved via the `start` and `create` methods of
[Actor](https://actix.github.io/actix/actix/trait.Actor.html) 
trait. It provides several different ways of creating actors, for details check docs. 
You can implement `started`, `stopping` and `stopped`mthods of Actor trait, 
`started` method get called when actor starts and `stopping` when actor finishes.
Check [api documentation](https://actix.github.io/actix/actix/trait.Actor.html#actor-lifecycle) 
for more information on actor lifecycle.

### Handle messages

An Actor communicates with another Actor by sending an messages. In actix all messages are typed.
Let's define simple `Sum` message with two `usize` parameters and actor which will
accept this message and return sum of those two numbers.

```rust
extern crate actix;
extern crate futures;
use futures::{future, Future};
use actix::*;

// this is our Message
struct Sum(usize, usize);

// we have to define type of response for `Sum` message
impl ResponseType for Sum {
    type Item = usize;
    type Error = ();
}

// Actor definition
struct Summator;

impl Actor for Summator {
    type Context = Context<Self>;
}

// now we need to define `MessageHandler` for `Sum` message.
impl Handler<Sum> for Summator {

    fn handle(&mut self, msg: Sum, ctx: &mut Context<Self>) -> Response<Self, Sum> {
        Self::reply(msg.0 + msg.1)
    }
}

fn main() {
    let system = System::new("test");

    let addr: Address<_> = Summator.start();

    // Address<A>::call() returns ActorFuture object, so we need to wait for result.
    // ActorFuture makes sense within Actor execution context, but we can use
    // Address<A>::call_fut() which return simple Future object.
    let res = addr.call_fut(Sum(10, 5));
    
    system.handle().spawn(res.then(|res| {
        match res {
            Ok(Ok(result)) => println!("SUM: {}", result),
            _ => println!("Something wrong"),
        }
        
        Arbiter::system().send(msgs::SystemExit(0));
        future::result(Ok(()))
    }));

    system.run();
}
```

All communications with actors go through `Address` object. You can `send` message
without waiting response or `call` actor with specific message. `ResponseType`
trait defines response type for a message, `Item` and `Error` for value and error respectevily.
There are different types of addresses.
[`Address<A>`](https://actix.github.io/actix/actix/struct.Address.html) is an address
of an actor that runs in same arbiter (event loop). If actor is running in different
thread [`SyncAddress<A>`](https://actix.github.io/actix/actix/struct.SyncAddress.html)
has to be used.

### Actor state and subscription for specific message

If you noticed, methods of `Actor` and `Handler` traits accept `&mut self`, so you are welcome to 
store anything in an actor and mutate it whenever you need.

Address object requires actor type, but if we just want to send specific message to 
an actor that can handle message, we can use `Subscriber` interface. Let's create
new actor that uses `Subscriber`, also this example will show how to use standard future objects.
Also in this example we are going to use unstable `proc_macro` rust's feature for message
and handler definitions

```rust
#![feature(proc_macro)]

extern crate actix;
use std::time::Duration;
use actix::*;

#[msg]
struct Ping { pub id: usize }

// Actor definition
struct Game {
    counter: usize, 
    addr: Box<Subscriber<Ping>>
}

#[actor(Context<_>)]
impl Game {

    #[simple(Ping)]
    // simple message handler for Ping message
    fn ping(&mut self, id: usize, ctx: &mut Context<Self>) {
        self.counter += 1;
        
        if self.counter > 10 {
            Arbiter::system().send(msgs::SystemExit(0));
        } else {
            println!("Ping received {:?}", id);
            
            // wait 100 nanos
            ctx.run_later(Duration::new(0, 100), move |act, _| {
                act.addr.send(Ping{id: id + 1});
            });
        }
    }
}

fn main() {
    let system = System::new("test");

    // we need Subscriber object so we need to use different builder method
    // which will allow to postpone actor creation
    let _: Address<_> = Game::create(|ctx| {
        // now we can get address of first actor and create second actor
        let addr: Address<_> = ctx.address();
        let addr2: Address<_> = Game{counter: 0, addr: addr.subscriber()}.start();
        
        // lets start pings
        addr2.send(Ping{id: 10});
        
        // now we can finally create first actor
        Game{counter: 0, addr: addr2.subscriber()}
    });

    system.run();
}
```

More information on signals handling is in
[signal](https://actix.github.io/actix/actix/actors/signal/index.html) module.


### chat example

You may consider to check 
[chat example](https://github.com/actix/actix/tree/master/examples/chat). 
It provides basic example of networking client/server service.


### fectl

You may consider to check [fectl](https://github.com/fafhrd91/fectl) utility. It is written
with `actix` and shows how to create networking application with relatevly complex interactions.

## Contributing

All contribution are welcome, if you have a feature request don't hesitate to open an issue!

## License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.
