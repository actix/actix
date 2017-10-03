# Actix [![Build Status](https://travis-ci.org/fafhrd91/actix.svg?branch=master)](https://travis-ci.org/fafhrd91/actix) [![crates.io](http://meritbadge.herokuapp.com/actix)](https://crates.io/crates/actix)

Actix is a rust actor system framework.

* [API Documentation (Development)](http://fafhrd91.github.io/actix/actix/)
* [API Documentation (Releases)](https://docs.rs/actix/)
* Cargo package: [actix](https://crates.io/crates/actix)

---

Actix is licensed under the [Apache-2.0 license](http://opensource.org/licenses/APACHE-2.0).

## Features

  * Typed messages (No `Any` type). Generic messages are allowed.
  * Actor communication in a local/thread context.
  * Actor supervision.
  * Using Futures for asynchronous message handling.
  * Compiles with stable rust 


## Usage

To use `actix`, add this to your `Cargo.toml`:

```toml
[dependencies]
actix = { git = "https://github.com/fafhrd91/actix.git" }
```


### Initiate the Actix

In order to use actix you first need to create an `System`.

```rust,ignore
extern crate actix;

fn main() {
    let system = actix::System::new("test".to_owned());
    
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
the [`Actor`](https://fafhrd91.github.io/actix/actix/trait.Actor.html) trait.


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
    let system = System::new("test".to_owned());

    let addr: Address<_> = MyActor.start();

    system.run();
    println!("Done");
}
```

Spawning a new actor is achieved via the `start` and `create` methods of
[Actor](https://fafhrd91.github.io/actix/actix/trait.Actor.html) 
trait. It provides several different ways of creating actors, for details check docs. 
You can implement `started`, `stopping` and `stopped`mthods of Actor trait, 
`started` method get called when actor starts and `stopping` when actor finishes.

### Handle messages

An Actor communicates with another Actor by sending an messages. In actix all messages are typed.
Let's define simple `Sum` message with two `usize` parameters and actor which will
accept this message and return sum of those two numbers.

```rust
extern crate actix;
extern crate futures;
use futures::{future, Future};
use actix::prelude::*;

// this is our Message
struct Sum(usize, usize);

// Actor definition
struct Summator;

impl Actor for Summator {
    type Context = Context<Self>;
}

// now we need to define `MessageHandler` for `Sum` message.
impl Handler<Sum> for Summator {

    fn handle(&mut self, msg: Sum, ctx: &mut Context<Self>) -> Response<Self, Sum> {
        Response::Reply(msg.0 + msg.1)
    }
}

// we have to define type of response for `Sum` message
impl ResponseType<Sum> for Summator {
    type Item = usize;
    type Error = ();
}

fn main() {
    let system = System::new("test".to_owned());

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
    println!("Done");
}
```

All communications with actors go through `Address` object. You can `send` message
without waiting response or `call` actor with specific message. `ResponseType`
trait defines response type for message, `Item` and `Error` for value and error respectevily.
There are different types of addresses.
[`Address<A>`](https://fafhrd91.github.io/actix/actix/struct.Address.html) is address
of an actor that runs in same arbiter (event loop). If actor is running in different
thread [`SyncAddress<A>`](https://fafhrd91.github.io/actix/actix/struct.SyncAddress.html)
has to be used.

### Actor state and subscription for specific message

If you noticed methods of `Actor` and `Handler` traits accept `&mut self`, so you are welcome to 
store anything in actor and mutate it whenever you need.

Address object requires actor type, but if we just want to send specific message to 
and actor that can handle message, we can use `Subscriber` interface. Let's create
new actor that uses `Subscriber`, also this example will show how to use standard future objects.

```rust
extern crate actix;
extern crate tokio_core;

use std::time::Duration;
use tokio_core::reactor::Timeout;

use actix::prelude::*;
use actix::actors::signal;

struct Ping;

// Actor definition
struct Game {
    counter: usize, 
    addr: Box<Subscriber<Ping>>
}

impl Actor for Game {
    type Context = Context<Self>;
}

// message handler for Ping message
impl Handler<Ping> for Game {

    fn handle(&mut self, msg: Ping, ctx: &mut Context<Self>) -> Response<Self, Ping> {
        self.counter += 1;
        
        if self.counter > 10 {
            Arbiter::system().send(msgs::SystemExit(0));
        } else {
            println!("Ping received");
            
            // wait 100 nanos
            Timeout::new(Duration::new(0, 100), Arbiter::handle())
                .unwrap()
                .actfuture() // if we want get access to actor state we have to use ActorFuture
                .then(|_, srv: &mut Game, ctx: &mut Context<Self>| {
                     srv.addr.send(Ping);
                     fut::ok(())
                 })
                 .spawn(ctx);
        }
        Response::Empty()
    }
}

impl ResponseType<Ping> for Game {
    type Item = ();
    type Error = ();
}

fn main() {
    let system = System::new("test".to_owned());

    // this is helper actor that manages unix signals, by default stops System
    let _: () = signal::DefaultSignalsHandler.start();

    // we need Subscriber object so we need to use different builder method
    // which will allow to postpone actor creation
    let _: Address<_> = Game::create(|ctx| {
        // now we can get address of first actor and create second actor
        let addr: Address<_> = ctx.address();
        let addr2: Address<_> = Game{counter: 0, addr: addr.subscriber()}.start();
        
        // lets start pings
        addr2.send(Ping);
        
        // now we can finally create first actor
        Game{counter: 0, addr: addr2.subscriber()}
    });

    system.run();
}
```

More information on signals handling is in
[signal](https://fafhrd91.github.io/actix/actix/actors/signal/index.html) module.

### fectl

You may consider to check [fectl](https://github.com/fafhrd91/fectl) utility. It is written
with `actix` and shows how to create networking application with relatevly complex interactions.

## Contributing

All contribution are welcome, if you have a feature request don't hesitate to open an issue!
