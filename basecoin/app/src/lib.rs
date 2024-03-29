//! The basecoin ABCI application.
#![forbid(unsafe_code)]
#![deny(
    warnings,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    rust_2018_idioms
)]

pub mod abci;
mod error;
pub mod service;

mod builder;
pub use builder::{BaseCoinApp, Builder};
