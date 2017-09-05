use std;

use version::PKG_INFO;
use client::ClientCommand;


pub fn load_config() -> Option<(ClientCommand, String)> {
    // cmd arguments
    let mut app = clap_app!(
        fectl =>
            (version: PKG_INFO.version)
            (author: PKG_INFO.authors)
            (about: PKG_INFO.description)
            (@arg sock: -m --sock + takes_value
             "Master process unix socket file path")
            (@arg command: "Run command (Supported commands: status, start, reload, restart, stop)")
            (@arg name: "Service name")
    );
    let mut help = Vec::new();
    let _ = app.write_long_help(&mut help);
    let args = app.get_matches();

    let command = args.is_present("command");
    if !command {
        print!("{}", String::from_utf8_lossy(&help));
        std::process::exit(0);
    }

    // check client args
    let cmd = args.value_of("command").unwrap();
    let sock = args.value_of("sock").unwrap_or("fectl.sock");
    match cmd.to_lowercase().trim() {
        "pid" =>
            return Some((ClientCommand::Pid, sock.to_owned())),
        "quit" =>
            return Some((ClientCommand::Quit, sock.to_owned())),
        "version" =>
            return Some((ClientCommand::Version, sock.to_owned())),
        _ => ()
    }

    if !args.is_present("name") {
        println!("Service name is required");
        return None
    }
    let name = args.value_of("name").unwrap().to_owned();

    let cmd = match cmd.to_lowercase().trim() {
        "status" => ClientCommand::Status(name),
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
    return Some((cmd, sock.to_owned()))
}
