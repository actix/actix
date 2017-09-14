use std;
use std::str::FromStr;

use time;
use env_logger::{self, LogBuilder};
use log::{LogRecord, LogLevelFilter};
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
            let formatter = |record: &LogRecord| {
                let t = time::now();
                format!("{},{:03} - {} - {}",
                        time::strftime("%Y-%m-%d %H:%M:%S", &t).unwrap(),
                        t.tm_nsec / 1000_000,
                        record.level(),
                        record.args()
                )
            };
            let _ = LogBuilder::new()
                .format(formatter)
                .filter(Some(PKG_INFO.name), level)
                .init();
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
