use crate::app::store::avl::AvlTree;
use crate::app::store::{Height, Path, ProvableStore, Store};

use std::convert::TryInto;

use ics23::CommitmentProof;
use tendermint::hash::Algorithm;
use tendermint::Hash;

type State = AvlTree<Vec<u8>, Vec<u8>>;

/// An in-memory store backed by an AvlTree.
#[derive(Clone)]
pub(crate) struct InMemoryStore {
    store: Vec<State>,
    pending: State,
}

impl Default for InMemoryStore {
    /// The store starts out by comprising the state of a single committed block, the genesis
    /// block, at height 0, with an empty state. We also initialize the pending location as empty.
    fn default() -> Self {
        InMemoryStore {
            store: vec![],
            pending: AvlTree::new(),
        }
    }
}

impl Store for InMemoryStore {
    type Error = ();

    fn set(&mut self, path: &Path, value: Vec<u8>) -> Result<(), Self::Error> {
        tracing::trace!("set at path = {}", path);
        self.pending.insert(path.0.clone().into_bytes(), value);
        Ok(())
    }

    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        tracing::trace!("get at path = {} at height = {:?}", path, height);
        match height {
            // Request to access the pending blocks
            Height::Pending => self.pending.get(path.0.as_bytes()).cloned(),
            // Access the last committed block
            Height::Latest => self.store.last().unwrap().get(path.0.as_bytes()).cloned(),
            // Access one of the committed blocks
            Height::Stable(height) => {
                let h = height as usize;
                if h < self.store.len() {
                    let state = self.store.get(h).unwrap();
                    state.get(path.0.as_bytes()).cloned()
                } else {
                    None
                }
            }
        }
    }

    fn delete(&mut self, _path: &Path) {
        todo!()
    }

    fn commit(&mut self) -> Vec<u8> {
        self.store.push(self.pending.clone());
        self.root_hash()
    }

    fn current_height(&self) -> u64 {
        self.store.len() as u64
    }

    fn get_keys(&self, key_prefix: Path) -> Vec<Path> {
        let keys = self.pending.get_keys();
        let key_prefix = key_prefix.0.into_bytes();
        keys.into_iter()
            .filter_map(|key| {
                key.starts_with(&key_prefix)
                    .then(|| key.as_slice().try_into().unwrap())
            })
            .collect()
    }
}

impl ProvableStore for InMemoryStore {
    fn root_hash(&self) -> Vec<u8> {
        self.pending
            .root_hash()
            .unwrap_or(&Hash::from_bytes(Algorithm::Sha256, &[0u8; 32]).unwrap())
            .as_bytes()
            .to_vec()
    }

    fn get_proof(&self, _key: &Path) -> Option<CommitmentProof> {
        todo!()
    }
}

// TODO: import tests
