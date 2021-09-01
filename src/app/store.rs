mod avl;
mod memory;

pub(crate) use memory::InMemoryStore;

use crate::app::modules::{Error as ModuleError, Identifiable};

use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};
use std::str::{from_utf8, Utf8Error};
use std::sync::{Arc, RwLock};

use flex_error::{define_error, TraceError};
use ics23::CommitmentProof;

/// A newtype representing a bytestring used as the key for an object stored in state.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Path(String);

impl Path {
    // TODO(hu55a1n1): clarify
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

impl TryFrom<String> for Path {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if Path::is_valid(&value) {
            Ok(Path(value))
        } else {
            Err(Error::invalid_path(value))
        }
    }
}

impl TryFrom<&[u8]> for Path {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let s = from_utf8(value).map_err(Error::malformed_path_string)?;
        s.to_owned().try_into()
    }
}

define_error! {
    #[derive(Eq, PartialEq)]
    Error {
        InvalidPath
            { path_str: String }
            | e | { format!("'{}' is not a valid path", e.path_str) },
        MalformedPathString
            [ TraceError<Utf8Error> ]
            | _ | { "path isn't a valid string" },

    }
}

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::store(e)
    }
}

/// Block height
pub(crate) type RawHeight = u64;

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
    type Error: core::fmt::Debug;

    /// Set `value` for `path`
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<(), Self::Error>;

    /// Get associated `value` for `path` at specified `height`
    fn get(&self, height: Height, path: Path) -> Option<Vec<u8>>;

    /// Delete specified `path`
    fn delete(&mut self, path: Path);

    /// Commit `Pending` block to canonical chain and create new `Pending`
    fn commit(&mut self) -> Result<Vec<u8>, Self::Error>;

    /// Apply accumulated changes to `Pending`
    fn apply(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Reset accumulated changes
    fn reset(&mut self) {}

    /// Prune historic blocks upto specified `height`
    fn prune(&self, height: RawHeight) -> Result<RawHeight, Self::Error> {
        Ok(height)
    }

    /// Return the current height of the chain
    fn current_height(&self) -> RawHeight;

    /// Return all keys that start with specified prefix
    fn get_keys(&self, key_prefix: Path) -> Vec<Path>; // TODO(hu55a1n1): implement support for all heights
}

/// ProvableStore trait
pub trait ProvableStore: Store {
    /// Return a vector commitment
    fn root_hash(&self) -> Vec<u8>;

    /// Return proof of existence for key
    fn get_proof(&self, key: Path) -> Option<ics23::CommitmentProof>;
}

#[derive(Clone)]
pub(crate) struct SharedSubStore<S, P> {
    store: Arc<RwLock<S>>,
    path: P,
}

impl<S, P> SharedSubStore<S, P> {
    pub(crate) fn new(store: Arc<RwLock<S>>, path: P) -> Self {
        Self { store, path }
    }
}

impl<S, P> Store for SharedSubStore<S, P>
where
    S: Store,
    P: Identifiable + Send + Sync + Clone,
{
    type Error = S::Error;

    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<(), Self::Error> {
        let mut store = self.store.write().unwrap();
        store.set(self.path.prefixed_path(path), value)
    }

    fn get(&self, height: Height, path: Path) -> Option<Vec<u8>> {
        let store = self.store.read().unwrap();
        store.get(height, self.path.prefixed_path(path))
    }

    fn delete(&mut self, path: Path) {
        let mut store = self.store.write().unwrap();
        store.delete(self.path.prefixed_path(path))
    }

    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        panic!("shared sub-stores may not commit!")
    }

    fn current_height(&self) -> RawHeight {
        let store = self.store.read().unwrap();
        store.current_height()
    }

    fn get_keys(&self, key_prefix: Path) -> Vec<Path> {
        let store = self.store.read().unwrap();
        store.get_keys(self.path.prefixed_path(key_prefix))
    }
}

impl<S, P> ProvableStore for SharedSubStore<S, P>
where
    S: ProvableStore,
    P: Identifiable + Send + Sync + Clone,
{
    fn root_hash(&self) -> Vec<u8> {
        let store = self.store.read().unwrap();
        store.root_hash()
    }

    fn get_proof(&self, key: Path) -> Option<CommitmentProof> {
        let store = self.store.read().unwrap();
        store.get_proof(key)
    }
}

pub(crate) trait PrefixedPath: Sized {
    fn prefixed_path(&self, s: Path) -> Path;
}

impl<T: Identifiable> PrefixedPath for T {
    fn prefixed_path(&self, s: Path) -> Path {
        if !s.0.starts_with(&format!("{}/", self.identifier())) {
            format!("{}/{}", self.identifier(), s).try_into().unwrap() // safety - path created by concatenation of two paths must be valid
        } else {
            s
        }
    }
}
