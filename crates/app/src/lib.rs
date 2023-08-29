//! The basecoin ABCI application.
#![deny(
    warnings,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    rust_2018_idioms
)]
#![forbid(unsafe_code)]
pub mod abci;
pub mod cli;
pub mod modules;
pub mod service;
pub mod types;
mod utils;

mod builder;
pub use builder::{BaseCoinApp, Builder};

mod runner;
pub use runner::default_app_runner;

pub(crate) const CHAIN_REVISION_NUMBER: u64 = 0;
