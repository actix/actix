extern crate env_logger;
#[macro_use] extern crate log;

extern crate serde_json;
#[macro_use] extern crate serde_derive;

extern crate structopt;
#[macro_use] extern crate structopt_derive;

extern crate chrono;
extern crate byteorder;
extern crate bytes;
extern crate tokio_io;

mod client;
mod config;
mod version {
    include!(concat!(env!("OUT_DIR"), "/version.rs"));
}
mod event {
    include!("../src/event.rs");
}
mod master_types {
    include!("../src/master_types.rs");
}


fn main() {
    let _ = env_logger::init();

    let success = match config::load_config() {
        Some((cmd, sock)) => client::run(cmd, sock),
        None => false,
    };
    std::process::exit(if success {0} else {1});
}
