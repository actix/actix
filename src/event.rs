use std;
use std::collections::VecDeque;
use std::time::{UNIX_EPOCH, SystemTime};

pub type ServiceStatus = (String, Vec<(String, Vec<Event>)>);

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum State {
    Starting,
    Reloading,
    Restarting,
    Running,
    StoppingOld,
    Stopping,
    Failed,
    Stopped,
    Paused,
    RestartFailed,
    ReloadFailed,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Reason {
    None,
    Initial,
    Exit,
    ConsoleRequest,
    WorkerRequest,
    SomeWorkersFailed,
    WorkerError(String),
    FailedToStart(Option<String>),
    HeartbeatFailed,
    StartupTimeout,
    StopTimeout,
    InitFailed,
    BootFailed,
    Signal(usize),
    ExitCode(i8),
    NewProcessDied,
    RestartFailedStartingWorker,
    RestartFailedRunningWorker,
    RestoreAftreFailed,
    ReloadAftreTimeout,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Event {
    pub state: State,
    pub reason: Reason,
    pub timestamp: u64,
    pub pid: Option<String>,
}

impl Event {

    pub fn new(state: State, reason: Reason, pid: Option<String>) -> Event {
        Event {
            state: state,
            reason: reason,
            pid: pid,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
        }
    }
}


pub struct Events {
    max: usize,
    events: VecDeque<Event>,
}

impl Events {
    /// Create new `Events`
    pub fn new(max: usize) -> Events {
        Events { max: max, events: VecDeque::new() }
    }

    /// Add new event
    pub fn add(&mut self, state: State, reason: Reason, pid: Option<String>) {
        if self.events.len() >= self.max {
            self.events.pop_front();
        }
        self.events.push_back(Event::new(state, reason, pid));
    }
}


impl<'a> std::convert::From<&'a Events> for Vec<Event>
{
    fn from(ob: &'a Events) -> Self {
        ob.events.iter().map(|e| e.clone()).collect()
    }
}
