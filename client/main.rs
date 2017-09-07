extern crate env_logger;
#[macro_use] extern crate log;

extern crate serde_json;
#[macro_use] extern crate serde_derive;

extern crate structopt;
#[macro_use] extern crate structopt_derive;

extern crate byteorder;
extern crate bytes;
extern crate tokio_io;

mod client;
mod config;
mod version {
    include!(concat!(env!("OUT_DIR"), "/version.rs"));
}

/// Master command
#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag="cmd", content="data")]
pub enum MasterRequest {
    /// Ping master process
    Ping,
    /// Status
    Status(String),
    /// Start service
    Start(String),
    /// Pause service
    Pause(String),
    /// Resume service
    Resume(String),
    /// Gracefully reload service
    Reload(String),
    /// Restart service
    Restart(String),
    /// Gracefully stop service
    Stop(String),
    /// Pid of the master process
    Pid,
    /// Quit process
    Quit,
    /// Version if the master
    Version,
}

/// Master responses
#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag="cmd", content="data")]
pub enum MasterResponse {
    Pong,
    Done,
    /// Pid of the master process
    Pid(String),
    /// Version of the master process
    Version(String),

    /// Service started
    ServiceStarted,
    /// Service Stopped
    ServiceStopped,
    /// Service failed, service is not available
    ServiceFailed,
    /// Service status
    ServiceStatus(String),

    /// System not ready
    ErrorNotReady,
    /// Service is unknown
    ErrorUnknownService,
    /// Service is starting
    ErrorServiceStarting,
    /// Service is running
    ErrorServiceRunning,
    /// Service is reloading
    ErrorServiceReloading,
    /// Service is stopping
    ErrorServiceStopping,
    /// Service is stopped
    ErrorServiceStopped,
    /// Service is failed
    ErrorServiceFailed,
}

fn main() {
    let _ = env_logger::init();

    let success = match config::load_config() {
        Some((cmd, sock)) => client::run(cmd, sock),
        None => false,
    };
    std::process::exit(if success {0} else {1});
}
