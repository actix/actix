//! Ring benchmark inspired by Programming Erlang: Software for a
//! Concurrent World, by Joe Armstrong, Chapter 8.11.2
//!
//! "Write a ring benchmark. Create N processes in a ring. Send a
//! message round the ring M times so that a total of N * M messages
//! get sent. Time how long this takes for different values of N and M."

use std::{env, io, time::SystemTime};

use actix::prelude::*;

/// A payload with a counter
#[derive(Debug, Message)]
#[rtype(result = "()")]
struct Payload(usize);

#[derive(Debug)]
struct Node {
    id: usize,
    limit: usize,
    next: Recipient<Payload>,
}

impl Actor for Node {
    type Context = Context<Self>;
}

impl Handler<Payload> for Node {
    type Result = ();

    fn handle(&mut self, msg: Payload, _: &mut Context<Self>) {
        if msg.0 >= self.limit {
            println!(
                "Actor {} reached limit of {} (payload was {})",
                self.id, self.limit, msg.0
            );

            System::current().stop();
            return;
        }

        // Some prime in order for different actors to report progress.
        // Large enough to print about once per second in debug mode.
        if msg.0 % 498989 == 1 {
            println!(
                "Actor {} received message {} of {} ({:.2}%)",
                self.id,
                msg.0,
                self.limit,
                100.0 * msg.0 as f32 / self.limit as f32
            );
        }

        self.next
            .do_send(Payload(msg.0 + 1))
            .expect("Unable to send payload");
    }
}

fn main() -> io::Result<()> {
    let sys = System::new();

    let (n_nodes, n_rounds) = parse_args();

    let now = SystemTime::now();

    sys.block_on(async {
        println!("Setting up {} nodes", n_nodes);
        let limit = n_nodes * n_rounds;

        let node = Node::create(move |ctx| {
            let first_addr = ctx.address();

            let mut prev_addr = Node {
                id: 1,
                limit,
                next: first_addr.recipient(),
            }
            .start();

            for id in 2..n_nodes {
                prev_addr = Node {
                    id,
                    limit,
                    next: prev_addr.recipient(),
                }
                .start();
            }

            Node {
                id: n_nodes,
                limit,
                next: prev_addr.recipient(),
            }
        });

        println!(
            "Sending start message and waiting for termination after {} messages...",
            limit
        );

        node.send(Payload(1)).await.unwrap();
    });

    sys.run().unwrap();

    match now.elapsed() {
        Ok(elapsed) => println!(
            "Time taken: {}.{:06} seconds ({} msg/second)",
            elapsed.as_secs(),
            elapsed.subsec_micros(),
            (n_nodes * n_rounds * 1000000) as u128 / elapsed.as_micros()
        ),
        Err(e) => println!("An error occurred: {:?}", e),
    }

    Ok(())
}

fn parse_args() -> (usize, usize) {
    let mut args = env::args();

    // skip first arg
    args.next();

    let n_nodes = args
        .next()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or_else(|| print_usage_and_exit());

    if n_nodes <= 1 {
        eprintln!("Number of nodes must be > 1");
        ::std::process::exit(1);
    }

    let n_rounds = args
        .next()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or_else(|| print_usage_and_exit());

    if args.next().is_some() {
        print_usage_and_exit();
    }

    (n_nodes, n_rounds)
}

fn print_usage_and_exit() -> ! {
    eprintln!("Usage:");
    eprintln!("cargo run --example ring -- <num-nodes> <num-times-message-around-ring>");
    ::std::process::exit(1);
}
