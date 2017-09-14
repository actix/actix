
extern crate time;
extern crate syslog;
extern crate env_logger;
#[macro_use] extern crate log;

extern crate structopt;
#[macro_use] extern crate structopt_derive;

extern crate serde;
extern crate serde_json;
#[macro_use] extern crate serde_derive;

extern crate byteorder;
extern crate toml;
extern crate mio;
extern crate nix;
extern crate net2;
extern crate libc;
extern crate bytes;
extern crate futures;
extern crate tokio_core;
extern crate tokio_signal;
extern crate tokio_uds;
extern crate tokio_io;

extern crate ctx;

mod addrinfo;
mod client;
mod config;
mod config_helpers;
mod cmd;
mod exec;
mod event;
mod logging;
mod master;
mod master_types;
mod service;
mod socket;
mod worker;
mod process;
mod io;
mod utils;

mod version {
    include!(concat!(env!("OUT_DIR"), "/version.rs"));
}

fn main() {
    let success = match config::load_config() {
        Some(cfg) => master::start(cfg),
        None => false,
    };
    std::process::exit(if success {0} else {1});
}
