//! The state of the basecoin ABCI application.

use crate::encoding::encode_varint;
use bytes::BytesMut;
use std::collections::HashMap;

const APP_HASH_LENGTH: usize = 16;

pub type AccountId = String;
pub type Account = u64;
pub type Store = HashMap<AccountId, Account>;

#[derive(Debug)]
pub struct BaseCoinState {
    // A mapping of account IDs to balances.
    store: Store,

    // The current height of the blockchain for this state.
    height: i64,

    // Memoized hash of our state.
    hash: Vec<u8>,
}

impl Default for BaseCoinState {
    fn default() -> Self {
        Self {
            store: Store::new(),
            height: 0,
            hash: vec![0_u8; APP_HASH_LENGTH],
        }
    }
}

impl BaseCoinState {
    /// Create application state, initializing it with the given store data.
    pub fn new(store: Store) -> Self {
        let mut state = Self {
            store,
            ..Self::default()
        };
        state.hash = compute_hash(&state.store);
        state
    }

    /// Retrieve the account with the given account ID.
    ///
    /// If it does not exist, returns `None`.
    pub fn get_account(&self, account_id: &str) -> Option<Account> {
        self.store.get(account_id).map(Clone::clone)
    }

    /// Upsert the given account.
    pub fn put_account(&mut self, account_id: &str, account: Account) {
        self.store.insert(account_id.to_string(), account);
    }

    /// Returns the height for our state.
    pub fn height(&self) -> i64 {
        self.height
    }

    /// Returns the hash of our current state.
    pub fn hash(&self) -> Vec<u8> {
        self.hash.clone()
    }

    /// Commits our state, returning the new height and hash.
    pub fn commit(&mut self) -> (i64, Vec<u8>) {
        self.height += 1;
        self.hash = compute_hash(&self.store);
        (self.height, self.hash())
    }
}

fn compute_hash(store: &HashMap<String, u64>) -> Vec<u8> {
    let mut hash = BytesMut::with_capacity(APP_HASH_LENGTH);
    encode_varint(store.len() as u64, &mut hash);
    hash.to_vec()
}
