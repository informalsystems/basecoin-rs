mod avl;
pub mod memory;

use crate::app::modules::IdentifiableBy;
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, RwLock};

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

impl Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for Path {
    type Error = PathError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if Path::is_valid(value) {
            Ok(Path(value.to_owned()))
        } else {
            Err(PathError::InvalidPath(value.to_owned()))
        }
    }
}
impl TryFrom<String> for Path {
    type Error = PathError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
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
pub trait Store: Send + Sync + Clone {
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

#[derive(Clone)]
pub(crate) struct SharedSubStore<S: Store, P: ToString> {
    pub(crate) store: Arc<RwLock<S>>,
    pub(crate) path: P,
}

impl<S: Store, P: IdentifiableBy<&'static str> + Sync + Clone + Send + Display> Store
    for SharedSubStore<S, P>
{
    type Error = S::Error;

    fn set(&mut self, path: &Path, value: Vec<u8>) -> Result<(), Self::Error> {
        let mut store = self.store.write().unwrap();
        store.set(&self.path.prefixed_path(path), value)
    }

    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        let store = self.store.read().unwrap();
        store.get(height, &self.path.prefixed_path(path))
    }

    fn delete(&mut self, path: &Path) {
        let mut store = self.store.write().unwrap();
        store.delete(&self.path.prefixed_path(path))
    }

    fn commit(&mut self) -> Vec<u8> {
        panic!("shared sub-stores may not commit!")
    }

    fn current_height(&self) -> RawHeight {
        let store = self.store.read().unwrap();
        store.current_height()
    }
}

pub(crate) trait PrefixedPath: Sized {
    fn prefixed_path(&self, s: &Path) -> Path;
}

impl<T: IdentifiableBy<&'static str>> PrefixedPath for T {
    fn prefixed_path(&self, s: &Path) -> Path {
        format!("{}/{}", self.identifier(), s).try_into().unwrap()
    }
}
