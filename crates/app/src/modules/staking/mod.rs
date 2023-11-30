mod impls;
mod service;

pub use impls::*;
pub use service::*;

/// Re-exports `staking` module proto types for convenience.
pub mod proto {
    pub use ibc_proto::cosmos::staking::*;
}
