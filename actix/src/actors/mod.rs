//! Helper actors

pub mod mocker;

#[cfg(feature = "resolver")]
#[deprecated(
    since = "0.11.0",
    note = "Resolver actor is deprecated and will be removed in a future release."
)]
pub mod resolver;
