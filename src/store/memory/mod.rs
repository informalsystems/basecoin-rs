mod avl;

use super::{Height, Path, ProvableStore, Store};
use avl::{AsBytes, AvlTree, ByteSlice};

use ics23::CommitmentProof;
use tendermint::hash::Algorithm;
use tendermint::Hash;
use tracing::trace;

// A state type that represents a snapshot of the store at every block.
// The value is a `Vec<u8>` to allow stored types to choose their own serde.
type State = AvlTree<Path, Vec<u8>>;

/// An in-memory store backed by an AvlTree.
#[derive(Clone)]
pub struct MemoryStore {
    /// collection of states corresponding to every committed block height
    store: Vec<State>,
    /// pending block state
    pending: State,
}

impl MemoryStore {
    #[inline]
    fn get_state(&self, height: Height) -> Option<&State> {
        match height {
            Height::Pending => Some(&self.pending),
            Height::Latest => self.store.last(),
            Height::Stable(height) => {
                let h = height as usize;
                if h <= self.store.len() {
                    self.store.get(h - 1)
                } else {
                    None
                }
            }
        }
    }
}

impl Default for MemoryStore {
    /// The store starts out with an empty state. We also initialize the pending location as empty.
    fn default() -> Self {
        Self {
            store: vec![],
            pending: AvlTree::new(),
        }
    }
}

impl Store for MemoryStore {
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
            .filter_map(|key| {
                key.as_bytes()
                    .as_ref()
                    .starts_with(key_prefix.as_ref())
                    .then(|| key.clone())
            })
            .collect()
    }
}

impl ProvableStore for MemoryStore {
    fn root_hash(&self) -> Vec<u8> {
        self.pending
            .root_hash()
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

impl AsBytes for Path {
    fn as_bytes(&self) -> ByteSlice<'_> {
        ByteSlice::Vector(self.to_string().into_bytes())
    }
}

// TODO(hu55a1n1): import tests
