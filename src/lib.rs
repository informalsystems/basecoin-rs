//! The basecoin library.
#![deny(
    warnings,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    rust_2018_idioms
)]
#![forbid(unsafe_code)]

mod application;
pub mod modules;
mod prostgen;
pub mod store;

pub mod prelude {
    use super::*;

    pub use application::Application;
    pub use modules::{auth::Auth, bank::Bank, ibc::Ibc, prefix, staking::Staking, *};
    pub use prostgen::cosmos::base::tendermint::v1beta1::service_server::ServiceServer as HealthServer;
    pub use prostgen::cosmos::tx::v1beta1::service_server::ServiceServer as TxServer;
    pub use prostgen::ibc::core::client::v1::query_server::QueryServer as ClientQueryServer;
    pub use prostgen::ibc::core::connection::v1::query_server::QueryServer as ConnectionQueryServer;
    pub use prostgen::ibc::core::port::v1::query_server::QueryServer as PortQueryServer;
    pub use store::MemoryStore;
}
