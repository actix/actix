use event::ServiceStatus;

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
    ServiceStatus(ServiceStatus),

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
