mod avl;
pub mod memory;

use crate::app::modules::{IdentifiableBy, Module};
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;

/// A newtype representing a bytestring used as the key for an object stored in state.
#[derive(Debug)]
pub struct Path(String);

impl Path {
    // TODO: clarify
    fn is_valid(s: impl AsRef<str>) -> bool {
        s.as_ref().chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '_' | '+' | '-' | '#' | '[' | ']' | '<' | '>' | '/')
        })
    }
}

impl TryFrom<String> for Path {
    type Error = PathError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if Path::is_valid(&value) {
            Ok(Path(value))
        } else {
            Err(PathError::InvalidPath(value))
        }
    }
}

#[derive(Debug)]
pub enum PathError {
    InvalidPath(String),
}

/// Block height
pub type RawHeight = u64;

/// Store height to query
#[derive(Debug)]
pub enum Height {
    Pending,
    Latest,
    Stable(RawHeight), // or equivalently `tendermint::block::Height`
}

impl From<RawHeight> for Height {
    fn from(value: u64) -> Self {
        match value {
            0 => Height::Latest,
            _ => Height::Stable(value),
        }
    }
}

/// Store trait - maybe provableStore or privateStore
pub trait Store: Send + Sync + Clone + Default {
    /// Error type - expected to envelope all possible errors in store
    type Error: StdError;

    /// Set `value` for `path`
    fn set(&mut self, path: &Path, value: Vec<u8>) -> Result<(), Self::Error>;

    /// Get associated `value` for `path` at specified `height`
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>>;

    /// Delete specified `path`
    fn delete(&mut self, path: &Path);

    /// Commit `Pending` block to canonical chain and create new `Pending`
    fn commit(&mut self) -> Vec<u8>;

    /// Prune historic blocks upto specified `height`
    fn prune(&self, height: RawHeight) -> Result<RawHeight, Self::Error> {
        Ok(height)
    }

    /// Return the current height of the chain
    fn current_height(&self) -> RawHeight;
}

/// ProvableStore trait
pub trait ProvableStore: Store {
    /// Return a vector commitment
    fn root_hash(&self) -> Vec<u8>;

    // Return proof of existence for key
    fn get_proof(&self, key: &Path) -> Option<ics23::CommitmentProof>;
}

pub(crate) trait PrefixedPath: Sized {
    fn prefixed_path(&self, s: &str) -> Path;
}

impl<T: Module + IdentifiableBy<&'static str>> PrefixedPath for T {
    fn prefixed_path(&self, s: &str) -> Path {
        format!("{}/{}", self.identifier(), s).try_into().unwrap()
    }
}
