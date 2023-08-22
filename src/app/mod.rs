pub mod abci;
pub mod service;

mod builder;
pub use builder::{BaseCoinApp, Builder};

mod runner;
pub use runner::default_app_runner;

pub(crate) const CHAIN_REVISION_NUMBER: u64 = 0;
