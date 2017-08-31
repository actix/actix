use std::env;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = env::var("OUT_DIR").expect("should not fail");
    let dst = PathBuf::from(out).join("version.rs");
    let mut f = std::fs::OpenOptions::new()
        .write(true).truncate(true).create(true).open(dst).expect("");

    f.write_all(format!(
"pub struct PkgInfo {{
    pub name: &'static str,
    pub description: &'static str,
    pub authors: &'static str,
    pub version: &'static str,
    pub version_major: u8,
    pub version_minor: u8,
    pub version_patch: u8,
    pub version_pre: &'static str,
}}

pub const PKG_INFO: PkgInfo = PkgInfo {{
    name: \"{}\",
    description: \"{}\",
    authors: \"{}\",
    version: \"{}\",
    version_major: {},
    version_minor: {},
    version_patch: {},
    version_pre: \"{}\",
}};",
        env::var("CARGO_PKG_NAME").unwrap_or("".to_owned()),
        env::var("CARGO_PKG_DESCRIPTION").unwrap_or("".to_owned()),
        env::var("CARGO_PKG_AUTHORS").unwrap_or("".to_owned()),
        env::var("CARGO_PKG_VERSION").unwrap_or("0.0.0".to_owned()),
        env::var("CARGO_PKG_VERSION_MAJOR").unwrap_or("0".to_owned()),
        env::var("CARGO_PKG_VERSION_MINOR").unwrap_or("0".to_owned()),
        env::var("CARGO_PKG_VERSION_PATCH").unwrap_or("0".to_owned()),
        env::var("CARGO_PKG_VERSION_PRE").unwrap_or("".to_owned()),
    ).as_bytes()).unwrap();
}
