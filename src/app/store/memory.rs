use crate::app::store::avl::{AsBytes, AvlTree};
use crate::app::store::{Height, Path, ProvableStore, Store};

use std::convert::TryInto;

use ics23::CommitmentProof;
use tendermint::hash::Algorithm;
use tendermint::Hash;

type State = AvlTree<Path, Vec<u8>>;

/// An in-memory store backed by an AvlTree.
#[derive(Clone)]
pub(crate) struct InMemoryStore {
    store: Vec<State>,
    pending: State,
}

impl Default for InMemoryStore {
    /// The store starts out with an empty state. We also initialize the pending location as empty.
    fn default() -> Self {
        Self {
            store: vec![],
            pending: AvlTree::new(),
        }
    }
}

impl Store for InMemoryStore {
    type Error = ();

    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<(), Self::Error> {
        tracing::trace!("set at path = {}", path);
        self.pending.insert(path, value);
        Ok(())
    }

    fn get(&self, height: Height, path: Path) -> Option<Vec<u8>> {
        tracing::trace!("get at path = {} at height = {:?}", &path, height);
        match height {
            // Request to access the pending blocks
            Height::Pending => self.pending.get(&path).cloned(),
            // Access the last committed block
            Height::Latest => self.store.last().and_then(|s| s.get(&path).cloned()),
            // Access one of the committed blocks
            Height::Stable(height) => {
                let h = height as usize;
                if h < self.store.len() {
                    let state = self.store.get(h).unwrap();
                    state.get(&path).cloned()
                } else {
                    None
                }
            }
        }
    }

    fn delete(&mut self, _path: Path) {
        todo!()
    }

    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        tracing::trace!("committing height: {}", self.store.len() + 1);
        self.store.push(self.pending.clone());
        Ok(self.root_hash())
    }

    fn current_height(&self) -> u64 {
        self.store.len() as u64
    }

    fn get_keys(&self, key_prefix: Path) -> Vec<Path> {
        let key_prefix = key_prefix.0.into_bytes();
        self.pending
            .get_keys()
            .into_iter()
            .filter_map(|key| {
                let key = key.0.as_bytes();
                key.starts_with(&key_prefix)
                    .then(|| key.try_into().unwrap())
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

    fn get_proof(&self, _key: Path) -> Option<CommitmentProof> {
        todo!()
    }
}

impl AsBytes for Path {
    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

// TODO(hu55a1n1): import tests
