pub mod memory;
mod avl;

use std::error::Error as StdError;

/// A newtype representing a bytestring used as the key for an object stored in state.
/// TODO: Must be validated in accordance with [ICS 24 - Path, Identifiers, Separators](https://github.com/cosmos/ibc/blob/0eba039ed65a66eace21124041d39be07ebfb69a/spec/core/ics-024-host-requirements/README.md#path-space)
pub struct Path(String);

/// Block height
pub type RawHeight = u64;

/// Store height to query
pub enum Height {
    Pending,
    Latest,
    Stable(RawHeight), // or equivalently `tendermint::block::Height`
}

/// Store trait - maybe provableStore or privateStore
pub trait Store {
    /// Error type - expected to envelope all possible errors in store
    type Error: StdError;

    /// Set `value` for `path`
    fn set(&mut self, path: &Path, value: Vec<u8>) -> Result<(), Self::Error>;

    /// Get associated `value` for `path` at specified `height`
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>>;

    /// Delete specified `path`
    fn delete(&mut self, path: &Path);

    /// Commit `Pending` block to canonical chain and create new `Pending`
    fn commit(&self) -> Vec<u8>;

    /// Prune historic blocks upto specified `height`
    fn prune(&self, height: RawHeight) -> Result<RawHeight, Self::Error> {
        Ok(height)
    }

    /// Return the current height of the chain
    fn current_height(&self) -> RawHeight;
}
