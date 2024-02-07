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
pub mod service;
mod utils;

mod builder;
pub use builder::{BaseCoinApp, Builder};

mod runner;
pub use runner::default_app_runner;

/// Re-exports the basecoin store types and implementations.
pub mod store {
    pub use basecoin_store::*;
}

/// Re-exports the basecoin modules.
pub mod modules {
    pub use basecoin_modules::*;
}
