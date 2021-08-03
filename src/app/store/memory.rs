use crate::app::store::avl::AvlTree;
use crate::app::store::{Height, Path, ProvableStore, Store};

use ics23::CommitmentProof;
use tendermint::hash::Algorithm;
use tendermint::Hash;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
pub enum Error {}

/// An in-memory store backed by an AvlTree.
#[derive(Clone)]
pub struct Memory {
    store: Vec<AvlTree<Vec<u8>, Vec<u8>>>,
    pending: AvlTree<Vec<u8>, Vec<u8>>,
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let last_store_keys = self.store.last().unwrap().get_keys();

        write!(
            f,
            "store::Memory {{ height: {}, keys: [{}] \n\tpending keys: [{}] }}",
            self.store.len(),
            last_store_keys
                .iter()
                .map(|k| String::from_utf8_lossy(k).into_owned())
                .collect::<Vec<String>>()
                .join(", "),
            self.pending
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
        Memory {
            store: vec![],
            pending: AvlTree::new(),
        }
    }
}

impl Store for Memory {
    type Error = Error;

    fn set(&mut self, path: &Path, value: Vec<u8>) -> Result<(), Self::Error> {
        self.pending.insert(path.0.clone().into_bytes(), value);
        Ok(())
    }

    fn get(&self, height: Height, path: &Path) -> Option<Vec<u8>> {
        println!("Memory get -> {:?} => {:?}", height, path);
        println!("Store: {:?}", &self);
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
        (self.store.len()) as u64
    }
}

impl ProvableStore for Memory {
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
