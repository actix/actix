/*
 * Ring benchmark inspired by Programming Erlang: Software for a
 * Concurrent World, by Joe Armstrong, Chapter 8.11.2
 *
 * "Write a ring benchmark. Create N processes in a ring. Send a
 * message round the ring M times so that a total of N * M messages
 * get sent. Time how long this takes for different values of N and M."
 */
use actix::*;

use std::env;
use std::time::SystemTime;

/// A payload with a counter
struct Payload(usize);

impl Message for Payload {
    type Result = ();
}

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

fn print_usage_and_exit() -> ! {
    eprintln!("Usage; actix-test <num-nodes> <num-times-message-around-ring>");
    ::std::process::exit(1);
}

fn main() -> std::io::Result<()> {
    let system = System::new("ring");

    let args = env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        print_usage_and_exit();
    }
    let n_nodes = if let Ok(arg_num_nodes) = args[1].parse::<usize>() {
        if arg_num_nodes <= 1 {
            eprintln!("Number of nodes must be > 1");
            ::std::process::exit(1);
        }
        arg_num_nodes
    } else {
        print_usage_and_exit();
    };

    let n_times = if let Ok(arg_ntimes) = args[2].parse::<usize>() {
        arg_ntimes
    } else {
        print_usage_and_exit()
    };

    let setup = SystemTime::now();

    println!("Setting up {} nodes", n_nodes);
    let limit = n_nodes * n_times;
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

    match setup.elapsed() {
        Ok(elapsed) => println!(
            "Time taken: {}.{:06} seconds",
            elapsed.as_secs(),
            elapsed.subsec_micros()
        ),
        Err(e) => println!("An error occured: {:?}", e),
    }
    println!(
        "Sending start message and waiting for termination after {} messages...",
        limit
    );
    let now = SystemTime::now();

    let _req = node.send(Payload(1));

    match system.run() {
        Ok(_) => println!("Completed"),
        Err(e) => println!("An error occured: {:?}", e),
    }

    match now.elapsed() {
        Ok(elapsed) => println!(
            "Time taken: {}.{:06} seconds ({} msg/second)",
            elapsed.as_secs(),
            elapsed.subsec_micros(),
            (n_nodes * n_times * 1000000) as u128 / elapsed.as_micros()
        ),
        Err(e) => println!("An error occured: {:?}", e),
    }

    Ok(())
}
