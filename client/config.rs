use structopt::StructOpt;
use client::ClientCommand;


#[derive(StructOpt, Debug)]
struct Cli {
    /// Master process unix socket file path
    #[structopt(long="sock", short="m", default_value="fectld.sock")]
    sock: String,

    /// Run command (Supported commands: status, start, reload, restart, stop)
    command: String,

    /// Service name
    name: Option<String>,
}


pub fn load_config() -> Option<(ClientCommand, String)> {
    // cmd arguments
    let args = Cli::from_args();
    let cmd = args.command.to_lowercase().trim().to_owned();
    let sock = args.sock.clone();

    // check client args
    match cmd.as_str() {
        "pid" =>
            return Some((ClientCommand::Pid, sock)),
        "quit" =>
            return Some((ClientCommand::Quit, sock)),
        "version" =>
            return Some((ClientCommand::Version, sock)),
        "version-check" =>
            return Some((ClientCommand::VersionCheck, sock)),
        _ => ()
    }

    let name = match args.name {
        None => {
            println!("Service name is required");
            return None
        }
        Some(ref name) =>
            name.clone()
    };

    let cmd = match cmd.as_str() {
        "status" => ClientCommand::Status(name),
        "spid" => ClientCommand::SPid(name),
        "start" => ClientCommand::Start(name),
        "stop" => ClientCommand::Stop(name),
        "reload" => ClientCommand::Reload(name),
        "restart" => ClientCommand::Restart(name),
        "pause" => ClientCommand::Pause(name),
        "resume" => ClientCommand::Resume(name),
        _ => {
            println!("Unknown command: {}", cmd);
            return None
        }
    };
    return Some((cmd, sock))
}
