mod avl;
mod memory;

pub(crate) use memory::InMemoryStore;

use crate::app::modules::Error as ModuleError;

use std::collections::VecDeque;
use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::str::{from_utf8, Utf8Error};
use std::sync::{Arc, RwLock};

use flex_error::{define_error, TraceError};
use ics23::CommitmentProof;
use tracing::trace;

/// A newtype representing a valid ICS024 identifier.
/// Implements `Deref<Target=String>`.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Identifier(String);

impl Identifier {
    /// Identifiers MUST be non-empty (of positive integer length).
    /// Identifiers MUST consist of characters in one of the following categories only:
    /// * Alphanumeric
    /// * `.`, `_`, `+`, `-`, `#`
    /// * `[`, `]`, `<`, `>`
    #[inline]
    fn is_valid(s: impl AsRef<str>) -> bool {
        let s = s.as_ref();
        if s.is_empty() {
            return false;
        }
        s.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '_' | '+' | '-' | '#' | '[' | ']' | '<' | '>' | '/')
        })
    }

    #[inline]
    fn unprefixed_path(&self, path: &Path) -> Path {
        // FIXME(hu55a1n1)
        path.clone()
    }
}

impl Deref for Identifier {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Identifier {
    type Error = Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if !Identifier::is_valid(&s) {
            Err(Error::invalid_identifier(s))
        } else {
            Ok(Self(s))
        }
    }
}

/// A newtype representing a valid ICS024 `Path`.
/// It is mainly used as the key for an object stored in state.
/// Implements `Deref<Target=String>`.
/// Paths MUST contain only `Identifier`s, constant strings, and the separator `/`
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Path(String);

impl Deref for Path {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Path {
    type Error = Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        // split will never return an empty iterator
        for id in s.split('/') {
            if !Identifier::is_valid(id) {
                return Err(Error::invalid_identifier(id.to_owned()));
            }
        }
        Ok(Self(s))
    }
}

impl TryFrom<&[u8]> for Path {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let s = from_utf8(value).map_err(Error::malformed_path_string)?;
        s.to_owned().try_into()
    }
}

impl From<Identifier> for Path {
    fn from(id: Identifier) -> Self {
        Self(id.0)
    }
}

define_error! {
    #[derive(Eq, PartialEq)]
    Error {
        InvalidIdentifier
            { identifier: String }
            | e | { format!("'{}' is not a valid identifier", e.identifier) },
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
#[derive(Debug, Copy, Clone)]
pub enum Height {
    Pending,
    Latest,
    Stable(RawHeight), // or equivalently `tendermint::block::Height`
}

impl From<RawHeight> for Height {
    fn from(value: u64) -> Self {
        match value {
            0 => Height::Latest, // see https://docs.tendermint.com/master/spec/abci/abci.html#query
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
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>>;

    /// Delete specified `path`
    fn delete(&mut self, path: &Path);

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
    fn get_keys(&self, key_prefix: &Path) -> Vec<Path>; // TODO(hu55a1n1): implement support for all heights
}

/// ProvableStore trait
pub trait ProvableStore: Store {
    /// Return a vector commitment
    fn root_hash(&self) -> Vec<u8>;

    /// Return proof of existence for key
    fn get_proof(&self, height: Height, key: &Path) -> Option<ics23::CommitmentProof>;
}

/// A wrapper store that implements a prefixed key-space for other shared stores
#[derive(Clone)]
pub(crate) struct SubStore<S> {
    /// backing store
    store: S,
    /// sub store
    sub_store: S,
    /// prefix for key-space
    prefix: Identifier,
    dirty: bool,
}

impl<S: Default + ProvableStore> SubStore<S> {
    pub(crate) fn new(store: S, prefix: Identifier) -> Result<Self, S::Error> {
        let mut sub_store = Self {
            store,
            sub_store: S::default(),
            prefix,
            dirty: false,
        };
        sub_store.update_parent_hash()?;
        Ok(sub_store)
    }

    pub(crate) fn prefix(&self) -> Identifier {
        self.prefix.clone()
    }
}

impl<S: Default + ProvableStore> SubStore<S> {
    fn update_parent_hash(&mut self) -> Result<(), S::Error> {
        self.store
            .set(Path::from(self.prefix.clone()), self.sub_store.root_hash())
    }
}

impl<S> Store for SubStore<S>
where
    S: Default + ProvableStore,
{
    type Error = S::Error;

    #[inline]
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<(), Self::Error> {
        self.dirty = true;
        self.sub_store
            .set(self.prefix.unprefixed_path(&path), value)
    }

    #[inline]
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        self.sub_store
            .get(height, &self.prefix.unprefixed_path(path))
    }

    #[inline]
    fn delete(&mut self, path: &Path) {
        self.dirty = true;
        self.sub_store.delete(&self.prefix.unprefixed_path(path))
    }

    #[inline]
    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        if self.dirty {
            self.dirty = false;
            self.update_parent_hash()?;
        }
        self.sub_store.commit()
    }

    #[inline]
    fn current_height(&self) -> RawHeight {
        self.sub_store.current_height()
    }

    #[inline]
    fn get_keys(&self, key_prefix: &Path) -> Vec<Path> {
        self.sub_store
            .get_keys(&self.prefix.unprefixed_path(key_prefix))
    }
}

impl<S> ProvableStore for SubStore<S>
where
    S: Default + ProvableStore,
{
    #[inline]
    fn root_hash(&self) -> Vec<u8> {
        self.sub_store.root_hash()
    }

    #[inline]
    fn get_proof(&self, height: Height, key: &Path) -> Option<CommitmentProof> {
        self.sub_store
            .get_proof(height, &self.prefix.unprefixed_path(key))
    }
}

/// Wraps a store to make it shareable by cloning
#[derive(Clone)]
pub(crate) struct SharedStore<S>(Arc<RwLock<S>>);

impl<S> SharedStore<S> {
    pub(crate) fn new(store: S) -> Self {
        Self(Arc::new(RwLock::new(store)))
    }
}

impl<S: Default + Store> Default for SharedStore<S> {
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<S: Store> Store for SharedStore<S> {
    type Error = S::Error;

    #[inline]
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<(), Self::Error> {
        self.write().unwrap().set(path, value)
    }

    #[inline]
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        self.read().unwrap().get(height, path)
    }

    #[inline]
    fn delete(&mut self, path: &Path) {
        self.write().unwrap().delete(path)
    }

    #[inline]
    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        self.write().unwrap().commit()
    }

    #[inline]
    fn current_height(&self) -> RawHeight {
        self.read().unwrap().current_height()
    }

    #[inline]
    fn get_keys(&self, key_prefix: &Path) -> Vec<Path> {
        self.read().unwrap().get_keys(key_prefix)
    }
}

impl<S: ProvableStore> ProvableStore for SharedStore<S> {
    #[inline]
    fn root_hash(&self) -> Vec<u8> {
        self.read().unwrap().root_hash()
    }

    #[inline]
    fn get_proof(&self, height: Height, key: &Path) -> Option<CommitmentProof> {
        self.read().unwrap().get_proof(height, key)
    }
}

impl<S> Deref for SharedStore<S> {
    type Target = Arc<RwLock<S>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> DerefMut for SharedStore<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// A wrapper store that implements rudimentary `apply()`/`reset()` support using write-ahead
/// logging for other stores
#[derive(Clone)]
pub(crate) struct WalStore<S> {
    /// backing store
    store: S,
    /// operation log for recording rollback operations in preserved order
    op_log: Vec<Path>,
}

impl<S: Store> WalStore<S> {
    pub(crate) fn new(store: S) -> Self {
        Self {
            store,
            op_log: vec![],
        }
    }
}

impl<S: Default + Store> Default for WalStore<S> {
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<S: Store> Store for WalStore<S> {
    type Error = S::Error;

    #[inline]
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<(), Self::Error> {
        self.store.set(path.clone(), value)?;
        self.op_log.push(path);
        Ok(())
    }

    #[inline]
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        self.store.get(height, path)
    }

    #[inline]
    fn delete(&mut self, _path: &Path) {
        unimplemented!("WALStore doesn't support delete operations yet!")
    }

    #[inline]
    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        // call `apply()` before `commit()` to make sure all operations are applied
        self.apply()?;
        self.store.commit()
    }

    #[inline]
    fn apply(&mut self) -> Result<(), Self::Error> {
        // note that we do NOT call the backing store's apply here - this allows users to create
        // multilayered `WalStore`s
        self.op_log.clear();
        Ok(())
    }

    #[inline]
    fn reset(&mut self) {
        // note that we do NOT call the backing store's reset here - this allows users to create
        // multilayered `WalStore`s
        trace!("Rollback operation log changes");
        while let Some(op) = self.op_log.pop() {
            self.store.delete(&op);
        }
    }

    #[inline]
    fn current_height(&self) -> u64 {
        self.store.current_height()
    }

    #[inline]
    fn get_keys(&self, key_prefix: &Path) -> Vec<Path> {
        self.store.get_keys(key_prefix)
    }
}

impl<S: ProvableStore> ProvableStore for WalStore<S> {
    #[inline]
    fn root_hash(&self) -> Vec<u8> {
        self.store.root_hash()
    }

    #[inline]
    fn get_proof(&self, height: Height, key: &Path) -> Option<CommitmentProof> {
        self.store.get_proof(height, key)
    }
}
