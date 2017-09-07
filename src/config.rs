use std;
use std::path::Path;
use std::error::Error;
use std::io::prelude::*;
use std::ffi::{CString, OsString};

use libc;
use nix;
use nix::unistd::{Gid, Uid};
use toml;
use serde;
use serde_json as json;
use structopt::StructOpt;

use socket;

pub struct Config {
    pub master: MasterConfig,
    pub sockets: Vec<socket::Socket>,
    pub logging: LoggingConfig,
    pub services: Vec<ServiceConfig>,
}

/// Master process configuration
///
/// ```toml
/// [master]
/// daemon = true
/// pid = "fectl.pid"
/// sock = "fectl.sock"
/// directory = "/path/to/dir"
/// ```
#[derive(Debug)]
pub struct MasterConfig {
    /// Start master process in daemon mode
    pub daemon: bool,
    /// Path to file with process pid
    pub pid: Option<OsString>,
    /// Path to controller unix domain socket
    pub sock: OsString,
    /// Change to specified directory before apps loading.
    pub directory: OsString,

    /// Set group id
    pub gid: Option<Gid>,
    /// Set uid id
    pub uid: Option<Uid>,

    /// Redirect stdout
    pub stdout: Option<String>,
    /// Redirect stderr
    pub stderr: Option<String>,
}

impl MasterConfig
{
    /// remove pid and sock files
    pub fn remove_files(&self) {
        if let Some(ref pid) = self.pid {
            let _ = std::fs::remove_file(pid);
        }
        let _ = std::fs::remove_file(&self.sock);
    }

    /// load pid of the master process
    pub fn load_pid(&self) -> Option<nix::unistd::Pid> {
        if let Some(ref pid) = self.pid {
            if let Ok(mut file) = std::fs::File::open(pid) {
                let mut buf = Vec::new();
                if let Ok(_) = file.read_to_end(&mut buf) {
                    let spid = String::from_utf8_lossy(buf.as_ref());
                    if let Ok(pid) = spid.parse::<i32>() {
                        return Some(nix::unistd::Pid::from_raw(pid))
                    }
                }
            }
        }
        None
    }

    /// save pid to filesystem
    pub fn save_pid(&self) -> Result<(), std::io::Error> {
        if let Some(ref pid) = self.pid {
            let mut file = std::fs::File::create(pid)?;
            file.write_all(nix::unistd::getpid().to_string().as_ref())?;
        }
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct TomlConfig {
    master: Option<TomlMasterConfig>,
    logging: Option<LoggingConfig>,
    #[serde(default = "default_vec")]
    socket: Vec<SocketConfig>,
    #[serde(default = "default_vec")]
    service: Vec<ServiceConfig>,
}

#[derive(Deserialize, Debug)]
struct TomlMasterConfig {
    #[serde(default = "default_sock")]
    pub sock: String,
    pub pid: Option<String>,
    pub directory: Option<String>,

    #[serde(default)]
    #[serde(deserialize_with="deserialize_gid_field")]
    pub gid: Option<Gid>,

    #[serde(default)]
    #[serde(deserialize_with="deserialize_uid_field")]
    pub uid: Option<Uid>,

    pub stdout: Option<String>,
    pub stderr: Option<String>,
}


#[derive(Deserialize, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum Proto {
    tcp4,
    tcp6,
    unix,
}

/// Socket configuration
///
/// ```toml
/// [[socket]]
/// name = "http"
/// port = 8080
/// ip = "0.0.0.0"
/// service = ["test"]
/// loader = "aiohttp"
/// arguments = ["arg1", "arg2", "arg3"]
/// ```
#[derive(Deserialize, Debug)]
pub struct SocketConfig {
    pub name: String,
    pub port: u32,
    pub host: Option<String>,
    #[serde(default = "default_backlog")]
    pub backlog: u16,
    #[serde(default = "default_proto")]
    pub proto: Proto,
    #[serde(default = "default_vec")]
    pub service: Vec<String>,
    pub app: Option<String>,
    #[serde(default = "default_vec")]
    pub arguments: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ServiceConfig {
    pub name: String,
    pub num: u32,
    pub command: String,

    /// Change to specified directory before service worker loading.
    pub directory: Option<String>,

    /// Switch worker process to run as this group.
    ///
    /// A valid group id (as an integer) or the name of a user that can be
    /// retrieved with a call to ``libc::getgrnam(value)`` or ``None`` to not
    /// change the worker processes group.
    #[serde(default)]
    #[serde(deserialize_with="deserialize_gid_field")]
    pub gid: Option<Gid>,

    /// Switch worker processes to run as this user.
    ///
    /// A valid user id (as an integer) or the name of a user that can be
    /// retrieved with a call to ``libc::getpwnam(value)`` or ``None`` to not
    /// change the worker process user.
    #[serde(default)]
    #[serde(deserialize_with="deserialize_uid_field")]
    pub uid: Option<Uid>,

    /// Workers silent for more than this many seconds are killed and restarted.
    ///
    /// Generally set to ten seconds. Only set this noticeably higher if
    /// you're sure of the repercussions for sync workers. For the non sync
    /// workers it just means that the worker process is still communicating and
    /// is not tied to the length of time required to handle a single request.
    #[serde(default="default_timeout")]
    pub timeout: u32,

    /// Timeout for worker startup.
    ///
    /// After start, workers have this much time to report radyness state.
    /// Workers that do not report `loaded` state to master are force killed and
    /// get restarted.
    #[serde(default="default_startup_timeout")]
    pub startup_timeout: u32,

    /// Timeout for graceful workers shutdown.
    ///
    /// After receiving a restart or stop signal, workers have this much time to finish
    /// serving requests. Workers still alive after the timeout (starting from
    /// the receipt of the restart signal) are force killed.
    #[serde(default="default_shutdown_timeout")]
    pub shutdown_timeout: u32,
}

/// Loging configuration
///
/// ```toml
/// [logging]
/// level = "info"
/// facility = "user"
/// ```
#[derive(Deserialize, Debug)]
pub struct LoggingConfig {
    pub name: String,
    pub service: String,
    pub level: Option<String>,
    pub facility: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            name: "default".to_owned(),
            service: "console".to_owned(),
            level: Some("info".to_owned()),
            facility: None,
        }
    }
}

fn default_vec<T>() -> Vec<T> {
    Vec::new()
}

fn default_sock() -> String {
    "fectl.sock".to_owned()
}

fn default_backlog() -> u16 {
    256
}

fn default_proto() -> Proto {
    Proto::tcp4
}

fn default_timeout() -> u32 {
    10
}

fn default_startup_timeout() -> u32 {
    30
}

fn default_shutdown_timeout() -> u32 {
    30
}

/// Deserialize `gid` field into `Gid`
pub(crate) fn deserialize_gid_field<'de, D>(de: D) -> Result<Option<Gid>, D::Error>
    where D: serde::Deserializer<'de>
{
    let deser_result: json::Value = serde::Deserialize::deserialize(de)?;
    match deser_result {
        json::Value::String(ref s) =>
            if let Ok(name) = CString::new(s.as_str()) {
                unsafe {
                    let ptr = libc::getgrnam(name.as_ptr());
                    return if ptr.is_null() {
                        Err(serde::de::Error::custom("Can not convert group name to group id"))
                    } else {
                        Ok(Some(Gid::from_raw((*ptr).gr_gid)))
                    };
                }
            } else {
                return Err(serde::de::Error::custom("Can not convert to plain string"))
            }
        json::Value::Number(num) => {
            if let Some(num) = num.as_u64() {
                if num <= u32::max_value() as u64 {
                    return Ok(Some(Gid::from_raw(num as libc::gid_t)))
                }
            }
        }
        _ => (),
    }
    Err(serde::de::Error::custom("Unexpected value"))
}

/// Deserialize `uid` field into `Uid`
fn deserialize_uid_field<'de, D>(de: D) -> Result<Option<Uid>, D::Error>
    where D: serde::Deserializer<'de>
{
    let deser_result: json::Value = serde::Deserialize::deserialize(de)?;
    match deser_result {
        json::Value::String(ref s) =>
            if let Ok(name) = CString::new(s.as_str()) {
                unsafe {
                    let ptr = libc::getpwnam(name.as_ptr());
                    return if ptr.is_null() {
                        Err(serde::de::Error::custom("Can not convert user name to user id"))
                    } else {
                        Ok(Some(Uid::from_raw((*ptr).pw_uid)))
                    };
                }
            } else {
                return Err(serde::de::Error::custom("Can not convert to plain string"))
            }
        json::Value::Number(num) => {
            if let Some(num) = num.as_u64() {
                if num <= u32::max_value() as u64 {
                    return Ok(Some(Uid::from_raw(num as u32)));
                }
            }
        }
        _ => (),
    }
    Err(serde::de::Error::custom("Unexpected value"))
}


#[derive(StructOpt, Debug)]
struct Cli {
    /// Sets a custom config file for fectld
    #[structopt(long="config", short="c", default_value="fectld.toml")]
    config: String,

    /// Run in background
    #[structopt(long="daemon", short="d")]
    daemon: bool,
}


pub fn load_config() -> Option<Config> {
    let args = Cli::from_args();

    println!("CFG: {:?}", args.config);
    let mut cfg_str = String::new();
    if let Err(err) = std::fs::File::open(args.config)
        .and_then(|mut f| f.read_to_string(&mut cfg_str))
    {
        println!("Can not read configuration file due to: {}", err.description());
        return None
    }

    let cfg: TomlConfig = match toml::from_str(&cfg_str) {
        Ok(cfg) => cfg,
        Err(err) => {
            println!("Can not parse config file: {}", err);
            return None
        }
    };

    // master config
    let toml_master = cfg.master.unwrap_or(TomlMasterConfig {
        sock: default_sock(),
        directory: None,
        pid: None,
        gid: None,
        uid: None,
        stdout: None,
        stderr: None,
    });

    // check if working directory exists
    let directory = if let Some(ref dir) = toml_master.directory {
        match std::fs::canonicalize(dir) {
            Ok(path) => path.into_os_string(),
            Err(err) => {
                println!("Error accessing working directory: {}", err);
                return None
            }
        }
    } else {
        match std::env::current_dir() {
            Ok(d) => d.into_os_string(),
            Err(_) => return None,
        }
    };

    // canonizalize pid path
    let pid = if let Some(pid) = toml_master.pid {
        Some(Path::new(&directory).join(&pid).into_os_string())
    } else {
        None
    };

    let master = MasterConfig {
        // set default value from command line
        daemon: args.daemon,

        // canonizalize socket path
        sock: Path::new(&directory).join(&toml_master.sock).into_os_string(),

        pid: pid,
        gid: toml_master.gid,
        uid: toml_master.uid,

        // check if working directory exists
        directory: directory,

        // redirect stdout/stdout to specifi files
        stdout: toml_master.stdout,
        stderr: toml_master.stderr,
    };

    // sockets config
    let sockets = match socket::Socket::load_config(&cfg.socket) {
        Ok(sockets) => sockets,
        Err(err) => {
            println!("{}", err);
            return None
        }
    };

    Some(Config {
        master: master,
        sockets: sockets,
        services: cfg.service,
        logging: cfg.logging.unwrap_or(LoggingConfig::default()),
    })
}
