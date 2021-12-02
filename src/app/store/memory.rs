use crate::app::store::avl::{AsBytes, AvlTree};
use crate::app::store::{Height, Path, ProvableStore, Store};

use ics23::CommitmentProof;
use tendermint::hash::Algorithm;
use tendermint::Hash;
use tracing::trace;

// A state type that represents a snapshot of the store at every block.
// The value is a `Vec<u8>` to allow stored types to choose their own serde.
type State = AvlTree<Path, Vec<u8>>;

/// An in-memory store backed by an AvlTree.
#[derive(Clone)]
pub(crate) struct InMemoryStore {
    /// collection of states corresponding to every committed block height
    store: Vec<State>,
    /// pending block state
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
    type Error = (); // underlying store ops are infallible

    fn set(&mut self, path: Path, value: Vec<u8>) -> Result<(), Self::Error> {
        trace!("set at path = {}", path.to_string());
        self.pending.insert(path, value);
        Ok(())
    }

    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        trace!(
            "get at path = {} at height = {:?}",
            path.to_string(),
            height
        );
        match height {
            // Request to access the pending block
            Height::Pending => self.pending.get(path).cloned(),
            // Access the last committed block
            Height::Latest => self.store.last().and_then(|s| s.get(path).cloned()),
            // Access one of the committed blocks
            Height::Stable(height) => {
                let h = height as usize;
                if h < self.store.len() {
                    let state = self.store.get(h).unwrap();
                    state.get(path).cloned()
                } else {
                    None
                }
            }
        }
    }

    fn delete(&mut self, _path: &Path) {
        todo!()
    }

    fn commit(&mut self) -> Result<Vec<u8>, Self::Error> {
        trace!("committing height: {}", self.store.len());
        self.store.push(self.pending.clone());
        Ok(self.root_hash())
    }

    fn current_height(&self) -> u64 {
        self.store.len() as u64
    }

    fn get_keys(&self, key_prefix: &Path) -> Vec<Path> {
        let key_prefix = key_prefix.as_bytes();
        self.pending
            .get_keys()
            .into_iter()
            .filter_map(|key| key.as_bytes().starts_with(key_prefix).then(|| key.clone()))
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

impl AsBytes for Path {
    fn as_bytes(&self) -> &[u8] {
        // self.as_str().as_bytes()
        todo!()
    }
}

// TODO(hu55a1n1): import tests
