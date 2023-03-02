use super::context::{ProvableStore, Store};
use crate::helper::{Height, Path, RawHeight};
use ics23::CommitmentProof;
use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock},
};
use tracing::trace;

/// Wraps a store to make it shareable by cloning
#[derive(Clone, Debug)]
pub struct SharedStore<S>(Arc<RwLock<S>>);

impl<S> SharedStore<S> {
    pub fn new(store: S) -> Self {
        Self(Arc::new(RwLock::new(store)))
    }

    pub fn share(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S> Default for SharedStore<S>
where
    S: Default + Store,
{
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<S> Store for SharedStore<S>
where
    S: Store,
{
    type Error = S::Error;

    #[inline]
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<Option<Vec<u8>>, Self::Error> {
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
    fn apply(&mut self) -> Result<(), Self::Error> {
        self.write().unwrap().apply()
    }

    #[inline]
    fn reset(&mut self) {
        self.write().unwrap().reset()
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

impl<S> ProvableStore for SharedStore<S>
where
    S: ProvableStore,
{
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

/// A wrapper store that implements rudimentary `apply()`/`reset()` support for other stores
#[derive(Clone, Debug)]
pub struct RevertibleStore<S> {
    /// backing store
    store: S,
    /// operation log for recording rollback operations in preserved order
    op_log: Vec<RevertOp>,
}

#[derive(Clone, Debug)]
enum RevertOp {
    Delete(Path),
    Set(Path, Vec<u8>),
}

impl<S> RevertibleStore<S>
where
    S: Store,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            op_log: vec![],
        }
    }
}

impl<S> Default for RevertibleStore<S>
where
    S: Default + Store,
{
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<S> Store for RevertibleStore<S>
where
    S: Store,
{
    type Error = S::Error;

    #[inline]
    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<Option<Vec<u8>>, Self::Error> {
        let old_value = self.store.set(path.clone(), value)?;
        match old_value {
            // None implies this was an insert op, so we record the revert op as delete op
            None => self.op_log.push(RevertOp::Delete(path)),
            // Some old value implies this was an update op, so we record the revert op as a set op
            // with the old value
            Some(ref old_value) => self.op_log.push(RevertOp::Set(path, old_value.clone())),
        }
        Ok(old_value)
    }

    #[inline]
    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        self.store.get(height, path)
    }

    #[inline]
    fn delete(&mut self, _path: &Path) {
        unimplemented!("RevertibleStore doesn't support delete operations yet!")
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
            match op {
                RevertOp::Delete(path) => self.delete(&path),
                RevertOp::Set(path, value) => {
                    self.set(path, value).unwrap(); // safety - reset failures are unrecoverable
                }
            }
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

impl<S> ProvableStore for RevertibleStore<S>
where
    S: ProvableStore,
{
    #[inline]
    fn root_hash(&self) -> Vec<u8> {
        self.store.root_hash()
    }

    #[inline]
    fn get_proof(&self, height: Height, key: &Path) -> Option<CommitmentProof> {
        self.store.get_proof(height, key)
    }
}
