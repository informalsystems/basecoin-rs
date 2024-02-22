use ics23::CommitmentProof;
use tendermint::hash::Algorithm;
use tendermint::Hash;
use tracing::trace;

use crate::avl::{AsBytes, AvlTree};
use crate::context::{ProvableStore, Store};
use crate::types::{Height, Path, RawHeight, State};

#[derive(Debug, Clone, Default)]
pub struct PrunedVec<T> {
    vec: Vec<T>,
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

    pub fn len(&self) -> usize {
        self.vec
            .len()
            .checked_add(self.pruned)
            .expect("no overflow")
    }

    pub fn prune(&mut self, index: usize) {
        trace!("pruning at index = {}", index);
        if index > self.pruned {
            self.vec.drain(0..index - self.pruned);
            self.pruned = index;
        }
    }
}

/// An in-memory store backed by an AvlTree.
#[derive(Clone, Debug)]
pub struct InMemoryStore {
    /// collection of states corresponding to every committed block height
    store: PrunedVec<State>,
    /// staged changes waiting to be committed
    /// these changes are from successful transactions
    staged: State,
    /// dirty changes that are not complete
    /// middle of a transaction which may fail
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
    type Error = (); // underlying store ops are infallible

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
        self.store.len() as u64
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
