use core::convert::Infallible;

use ics23::CommitmentProof;
use tendermint::hash::Algorithm;
use tendermint::Hash;
use tracing::trace;

use crate::avl::{AsBytes, AvlTree};
use crate::context::{ProvableStore, Store};
use crate::types::{Height, Path, RawHeight, State};

/// A wrapper type around [`Vec`] that more easily facilitates the pruning of
/// its elements at a particular height / index. Keeps track of the latest
/// height at which its elements were pruned.
///
/// This type is used by [`InMemoryStore`] in order to prune old store entries.
#[derive(Debug, Clone, Default)]
pub struct PrunedVec<T> {
    vec: Vec<T>,
    /// The latest index at which elements were pruned. In other words,
    /// elements that exist at and before this index are no longer accessible.
    pruned: usize,
}

impl<T> PrunedVec<T> {
    pub fn push(&mut self, value: T) {
        self.vec.push(value);
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.vec.get(index.checked_sub(self.pruned)?)
    }

    pub fn last(&self) -> Option<&T> {
        self.vec.last()
    }

    /// Returns the number of elements currently in the `PrunedVec`,
    /// i.e., the total number of elements minus the pruned elements.
    pub fn current_length(&self) -> usize {
        self.vec.len()
    }

    /// Returns the number of elements that have been pruned over the
    /// lifetime of the instance of this type.
    pub fn pruned_length(&self) -> usize {
        self.pruned
    }

    /// Returns the total number of elements that have been added to
    /// the `PrunedVec` over the lifetime of the instance of this type.
    /// This includes the number of pruned elements in its count.
    pub fn original_length(&self) -> usize {
        self.current_length() + self.pruned_length()
    }

    /// Removes all elements from the `PrunedVec` up to the specified
    /// index, inclusive. Note that `index` needs to be strictly greater
    /// than the current `self.pruned` index, otherwise this method is
    /// a no-op.
    pub fn prune(&mut self, index: usize) {
        trace!("pruning at index = {}", index);
        if index > self.pruned {
            self.vec.drain(0..index - self.pruned);
            self.pruned = index;
        }
    }
}

/// An in-memory store backed by an AvlTree.
///
/// [`InMemoryStore`] has two copies of the current working store - `staged` and `pending`.
///
/// Each transaction works on the `pending` copy. When a transaction returns:
/// - If it succeeded, the store _applies_ the transaction changes by copying `pending` to `staged`.
/// - If it failed, the store _reverts_ the transaction changes by copying `staged` to `pending`.
///
/// When a block is committed, the staged copy is copied into the committed store.
///
/// Note that this store implementation is not production-friendly. After each transaction,
/// the entire store is copied from `pending` to `staged`, or from `staged` to `pending`.
#[derive(Clone, Debug)]
pub struct InMemoryStore {
    /// A collection of states corresponding to every committed block height.
    store: PrunedVec<State>,
    /// The changes made as a result of successful transactions that are staged
    /// and waiting to be committed.
    staged: State,
    /// The dirty changes resulting from transactions that have not yet completed.
    pending: State,
}

impl InMemoryStore {
    #[inline]
    fn get_state(&self, height: Height) -> Option<&State> {
        match height {
            Height::Pending => Some(&self.pending),
            Height::Latest => self.store.last(),
            Height::Stable(height) => {
                if height == 0 {
                    None
                } else {
                    let h = height as usize;
                    self.store.get(h - 1)
                }
            }
        }
    }
}

impl Default for InMemoryStore {
    /// The store starts out with an empty state. We also initialize the pending location as empty.
    fn default() -> Self {
        let genesis_state = AvlTree::new();

        let store = PrunedVec::default();
        let staged = genesis_state.clone();
        let pending = genesis_state.clone();

        Self {
            store,
            staged,
            pending,
        }
    }
}

impl Store for InMemoryStore {
    type Error = Infallible;

    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<Option<Vec<u8>>, Self::Error> {
        trace!("set at path = {}", path.to_string());
        Ok(self.pending.insert(path, value))
    }

    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        trace!(
            "get at path = {} at height = {:?}",
            path.to_string(),
            height
        );
        self.get_state(height).and_then(|v| v.get(path).cloned())
    }

    fn delete(&mut self, _path: &Path) {
        todo!()
    }

    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        self.apply()?;
        trace!("committing height: {}", self.current_height());
        self.store.push(self.staged.clone());
        Ok(self.root_hash())
    }

    fn apply(&mut self) -> Result<(), Self::Error> {
        trace!("applying height: {}", self.current_height());
        self.staged = self.pending.clone();
        Ok(())
    }

    fn reset(&mut self) {
        trace!("resetting height: {}", self.current_height());
        self.pending = self.staged.clone();
    }

    fn prune(&mut self, height: RawHeight) -> Result<RawHeight, Self::Error> {
        let h = height as usize;
        self.store.prune(h);
        Ok(height)
    }

    fn current_height(&self) -> u64 {
        self.store.original_length() as u64
    }

    fn get_keys(&self, key_prefix: &Path) -> Vec<Path> {
        let key_prefix = key_prefix.as_bytes();
        self.pending
            .get_keys()
            .into_iter()
            .filter(|&key| key.as_bytes().as_ref().starts_with(key_prefix.as_ref()))
            .cloned()
            .collect()
    }
}

impl ProvableStore for InMemoryStore {
    fn root_hash(&self) -> Vec<u8> {
        self.get_state(Height::Latest)
            .and_then(|s| s.root_hash())
            .unwrap_or(&Hash::from_bytes(Algorithm::Sha256, &[0u8; 32]).unwrap())
            .as_bytes()
            .to_vec()
    }

    fn get_proof(&self, height: Height, key: &Path) -> Option<CommitmentProof> {
        trace!(
            "get proof at path = {} at height = {:?}",
            key.to_string(),
            height
        );
        self.get_state(height).and_then(|v| v.get_proof(key))
    }
}

// TODO(hu55a1n1): import tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pruned_vec() {
        let mut pv = PrunedVec::default();
        pv.push(1);
        pv.push(2);
        pv.push(3);
        pv.push(4);
        pv.push(5);
        assert_eq!(pv.original_length(), 5);
        pv.prune(2);
        assert_eq!(pv.original_length(), 5);
        assert_eq!(pv.pruned_length(), 2);
        assert_eq!(pv.current_length(), 3);
        assert_eq!(pv.get(0), None);
        assert_eq!(pv.get(1), None);
        assert_eq!(pv.get(2), Some(&3));
        assert_eq!(pv.get(3), Some(&4));
        assert_eq!(pv.get(4), Some(&5));
        assert_eq!(pv.get(5), None);
        assert_eq!(pv.last(), Some(&5));
    }

    #[test]
    fn test_in_memory_store() {
        let mut store = InMemoryStore::default();
        assert!(!store.root_hash().is_empty());
        assert_eq!(store.current_height(), 0);

        let path = Path::from("a".to_owned());
        let value1 = vec![1, 2, 3];
        let value2 = vec![4, 5, 6];

        store.set(path.clone(), value1.clone()).unwrap();
        assert_eq!(store.get(Height::Pending, &path), Some(value1.clone()));
        assert_eq!(store.get(Height::Latest, &path), None);
        assert_eq!(store.get(Height::Stable(1), &path), None);

        store.apply().unwrap();
        store.commit().unwrap();

        assert_eq!(store.get(Height::Pending, &path), Some(value1.clone()));
        assert_eq!(store.get(Height::Latest, &path), Some(value1.clone()));
        assert_eq!(store.get(Height::Stable(1), &path), Some(value1.clone()));
        assert_eq!(store.get(Height::Stable(2), &path), None);
        assert_eq!(store.current_height(), 1);
        assert!(!store.root_hash().is_empty());

        store.set(path.clone(), value2.clone()).unwrap();
        assert_eq!(store.get(Height::Pending, &path), Some(value2.clone()));
        assert_eq!(store.get(Height::Latest, &path), Some(value1.clone()));
        assert_eq!(store.get(Height::Stable(1), &path), Some(value1.clone()));

        store.apply().unwrap();
        store.commit().unwrap();

        assert_eq!(store.get(Height::Pending, &path), Some(value2.clone()));
        assert_eq!(store.get(Height::Latest, &path), Some(value2.clone()));
        assert_eq!(store.get(Height::Stable(1), &path), Some(value1.clone()));
        assert_eq!(store.get(Height::Stable(2), &path), Some(value2.clone()));
        assert_eq!(store.get(Height::Stable(3), &path), None);
        assert_eq!(store.current_height(), 2);
        assert!(!store.root_hash().is_empty());
    }
}
