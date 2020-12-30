//! Helper actors

pub mod mocker;

#[cfg(feature = "resolver")]
#[deprecated(
    since = "0.20.0",
    note = "resolver actor is deprecated and possibly removed in future release."
)]
pub mod resolver;
