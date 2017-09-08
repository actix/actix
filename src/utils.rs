use std::env;
use std::path::Path;
use std::ffi::CString;

use nix::unistd::Pid;


/// find file in `PATH` environ
pub(crate) fn find_path(name: &str) -> Option<String>
{
    let path = Path::new(name);
    if path.is_file() {
        return Some(path.to_string_lossy().as_ref().to_owned())
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).filter_map(|dir| {
            let full_path = dir.join(&path);
            if full_path.is_file() {
                Some(full_path.to_string_lossy().as_ref().to_owned())
            } else {
                None
            }
        }).next()
    })
}

pub fn get_env_vars(all: bool) -> Vec<CString> {
    let mut env = Vec::new();
    for (k, v) in env::vars() {
        if k.starts_with("FECTL_FD") || k.starts_with("LANG") || k.starts_with("LC_") {
            env.push(CString::new(format!("{}={}", k, v)).unwrap());
        }
        else if all && !k.starts_with("_") {
            env.push(CString::new(format!("{}={}", k, v)).unwrap());
        }
    }
    env
}


pub fn str(pid: Pid) -> Option<String> {
    Some(format!("{}", pid))
}
