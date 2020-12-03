//! A configurable source of time.
//!
//! This module provides an API to get the current instant in such a way that
//! the source of time may be configured. This allows mocking out the source of
//! time in tests.
//!
//! See [Module `tokio_timer::clock`] for full documentation.
//!
//! [Module `tokio_timer::clock`]: https://docs.rs/tokio-timer/latest/tokio_timer/clock/index.html

// TODO: use the re-export of actix-rt?
pub use tokio::time::{
    interval_at, sleep, sleep_until, Duration, Instant, Interval, Sleep,
};
