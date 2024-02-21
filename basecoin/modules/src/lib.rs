#![forbid(unsafe_code)]
#![deny(
    warnings,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    rust_2018_idioms
)]

pub mod auth;
pub mod bank;
pub mod context;
pub mod error;
pub mod gov;
pub mod ibc;
pub mod staking;
pub mod types;
pub mod upgrade;

pub const CHAIN_REVISION_NUMBER: u64 = 0;
