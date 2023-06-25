//! The basecoin ABCI application.
#![deny(
    warnings,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    rust_2018_idioms
)]
#![forbid(unsafe_code)]
pub mod app;
pub mod cli;
pub mod config;
pub mod error;
mod helper;
pub mod modules;
pub mod store;
