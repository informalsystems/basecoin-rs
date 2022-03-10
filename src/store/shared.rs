use super::{Height, Path, Proof, ProvableStore, RawHeight, Store};

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock};

/// Wraps a store to make it shareable by cloning
#[derive(Clone)]
pub struct SharedStore<S>(Arc<RwLock<S>>);

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

impl<S: ProvableStore> ProvableStore for SharedStore<S> {
    #[inline]
    fn root_hash(&self) -> Vec<u8> {
        self.read().unwrap().root_hash()
    }

    #[inline]
    fn get_proof(&self, height: Height, key: &Path) -> Option<Proof> {
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
