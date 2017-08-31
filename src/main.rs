#[macro_use] extern crate log;
#[macro_use] extern crate clap;
#[macro_use] extern crate serde_derive;
extern crate byteorder;
extern crate serde;
extern crate serde_json;
extern crate toml;
extern crate syslog;
extern crate env_logger;
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
mod config;
mod client;
mod cmd;
mod exec;
mod logging;
mod master;
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
        config::ConfigInfo::Server(cfg) => master::start(cfg),
        config::ConfigInfo::Client(cmd, sock) => {
            std::env::set_var("RUST_LOG", "info");
            logging::init_logging(&config::LoggingConfig::default());
            client::run(cmd, sock)
        }
        config::ConfigInfo::Error => false
    };
    std::process::exit(if success {0} else {1});
}
