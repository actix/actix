use std::ffi::CString;

use libc;
use nix::unistd::{Gid, Uid};
use serde;
use serde_json as json;

use config::Proto;


pub fn default_vec<T>() -> Vec<T> {
    Vec::new()
}

pub fn default_sock() -> String {
    "fectld.sock".to_owned()
}

pub fn default_backlog() -> u16 {
    256
}

pub fn default_proto() -> Proto {
    Proto::tcp4
}

pub fn default_restarts() -> u16 {
    3
}

pub fn default_timeout() -> u32 {
    10
}

pub fn default_startup_timeout() -> u32 {
    30
}

pub fn default_shutdown_timeout() -> u32 {
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
pub fn deserialize_uid_field<'de, D>(de: D) -> Result<Option<Uid>, D::Error>
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
