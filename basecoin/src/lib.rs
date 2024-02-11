#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![deny(
    warnings,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    rust_2018_idioms
)]

pub mod cli;
pub mod config;
pub mod helper;
mod runner;

pub use runner::default_app_runner;

/// Re-exports Basecoin's store types and implementations.
pub mod store {
    pub use basecoin_store::*;
}

/// Re-exports Basecoin's modules types and implementations.
pub mod modules {
    pub use basecoin_modules::*;
}

/// Re-exports Basecoin's ABCI application types and implementations.
pub mod app {
    pub use basecoin_app::*;
}
