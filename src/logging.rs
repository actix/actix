use std;
use std::str::FromStr;

use env_logger;
use log::LogLevelFilter;
use syslog::{self, Facility};

use version::PKG_INFO;
use config::LoggingConfig;

enum Service {
    Console,
    Syslog,
}

pub fn init_logging(cfg: &LoggingConfig) {
    let srv = if cfg.service == "console" {
        Service::Console
    } else {
        Service::Syslog
    };

    let level = cfg.level.as_ref().and_then(
        |s| match LogLevelFilter::from_str(&s) {
            Ok(lvl) => Some(lvl),
            Err(_) => {
                println!("Can not parse log level value, using `info` level");
                Some(LogLevelFilter::Info)
            }
        }).unwrap_or(LogLevelFilter::Info);

    let facility = cfg.facility.as_ref().and_then(
        |s| match Facility::from_str(&s) {
            Ok(val) => Some(val),
            Err(_) => {
                println!("Can not parse log facility value, using `USER`");
                Some(Facility::LOG_USER)
            }
        }).unwrap_or(Facility::LOG_USER);

    match srv {
        Service::Console => {
            std::env::set_var("RUST_LOG", format!("{}={}", PKG_INFO.name, level));
            let _ = env_logger::init();
        }
        Service::Syslog => {
            if !syslog::init(facility, level, Some(cfg.name.as_ref())).is_ok() {
                std::env::set_var("RUST_LOG", format!("{}={}", PKG_INFO.name, level));
                let _ = env_logger::init();
                warn!("Could not initialize syslog");
            }
        }
    }
}
