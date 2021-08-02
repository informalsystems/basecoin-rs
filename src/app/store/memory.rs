use std::sync::RwLock;

use crate::app::store::avl::AvlTree;
use crate::app::store::{Height, Path, ProvableStore, Store};
// use crate::encoding::encode_varint;

use ics23::CommitmentProof;
use thiserror::Error as ThisError;
// use bytes::BytesMut;
use tendermint::hash::Algorithm;
use tendermint::Hash;

#[derive(ThisError, Debug)]
pub enum Error {}

/// An in-memory store backed by an AvlTree.
pub struct Memory {
    store: RwLock<Vec<AvlTree<Vec<u8>, Vec<u8>>>>,
    pending: RwLock<AvlTree<Vec<u8>, Vec<u8>>>,
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let store = self.store.read().unwrap();
        let pending = self.pending.read().unwrap();
        let last_store_keys = store.last().unwrap().get_keys();

        write!(
            f,
            "store::Memory {{ height: {}, keys: [{}] \n\tpending keys: [{}] }}",
            store.len(),
            last_store_keys
                .iter()
                .map(|k| String::from_utf8_lossy(k).into_owned())
                .collect::<Vec<String>>()
                .join(", "),
            pending
                .get_keys()
                .iter()
                .map(|k| String::from_utf8_lossy(k).into_owned())
                .collect::<Vec<String>>()
                .join(", ")
        )
    }
}

impl Default for Memory {
    /// The store starts out by comprising the state of a single committed block, the genesis
    /// block, at height 0, with an empty state. We also initialize the pending location as empty.
    fn default() -> Self {
        let genesis = AvlTree::new();
        let pending = genesis.clone();

        Memory {
            store: RwLock::new(vec![genesis]),
            pending: RwLock::new(pending),
        }
    }
}

impl Store for Memory {
    type Error = Error;

    fn set(&mut self, path: &Path, value: Vec<u8>) -> Result<(), Self::Error> {
        let mut store = self.pending.write().unwrap();
        store.insert(path.0.clone().into_bytes(), value);
        Ok(())
    }

    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        let store = self.store.read().unwrap();

        match height {
            // Request to access the pending blocks
            Height::Pending => {
                drop(store); // Release lock on the stable store
                let pending = self.pending.read().unwrap();
                pending.get(path.0.as_bytes()).cloned()
            }
            // Access the last committed block
            Height::Latest => {
                // Access the last committed block
                return store.last().unwrap().get(path.0.as_bytes()).cloned();
            }
            // Access one of the committed blocks
            Height::Stable(height) => {
                let h = height as usize;
                if h < store.len() {
                    let state = store.get(h).unwrap();
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
        let mut store = self.store.write().unwrap();
        let pending = self.pending.write().unwrap();
        let pending_copy = pending.clone();
        store.push(pending_copy);
        // pending.root_hash().unwrap().as_bytes().to_vec()
        self.root_hash()
    }

    fn current_height(&self) -> u64 {
        let store = self.store.read().unwrap();
        store.len() as u64
    }
}

impl ProvableStore for Memory {
    fn root_hash(&self) -> Vec<u8> {
        let pending = self.pending.read().unwrap();
        pending
            .root_hash()
            .unwrap_or(&Hash::from_bytes(Algorithm::Sha256, &[0u8; 16]).unwrap())
            .as_bytes()
            .to_vec()
    }

    fn get_proof(&self, _key: &Path) -> Option<CommitmentProof> {
        todo!()
    }
}

// TODO: import tests
