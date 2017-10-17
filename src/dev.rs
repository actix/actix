//! The `actix` prelude for library developers
//!
//! The purpose of this module is to alleviate imports of many common actix traits
//! by adding a glob import to the top of actix heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use actix::dev::*;
//! ```
pub use prelude::*;

pub use address::{ActorAddress};
pub use context::{AsyncContextApi, ActorAddressCell, ActorItemsCell, ActorWaitCell};
