//! The state of the basecoin ABCI application.

use crate::encoding::encode_varint;
use crate::result::Result;
use bytes::BytesMut;
use cosmos_sdk::Coin;
use flex_error::ErrorMessageTracer;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use thiserror::Error;

const APP_HASH_LENGTH: usize = 16;

/// Unique identifiers for accounts.
pub type AccountId = String;

/// A currency denomination.
pub type Denom = String;

/// A mapping of currency denomination identifiers to balances.
pub type Balances = HashMap<Denom, u64>;

/// Our account store.
#[derive(Debug, Clone, Deserialize)]
pub struct Store(HashMap<AccountId, Account>);

impl Store {
    fn get(&self, account_id: &str) -> Option<&Account> {
        self.0.get(account_id)
    }

    fn insert(&mut self, account_id: AccountId, account: Account) {
        self.0.insert(account_id, account);
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

impl Default for Store {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

#[derive(Debug, Clone, Error)]
pub enum StateError {
    #[error("no such account with ID {0}")]
    NoSuchAccount(String),
    #[error("insufficient funds in source account")]
    InsufficientSourceFunds,
    #[error("destination account fund overflow")]
    DestFundOverflow,
    #[error("unable to parse coin amount: {0} {0}")]
    CannotParseCoinAmount(String, String),
}

/// A single account's details.
#[derive(Debug, Clone, Deserialize)]
pub struct Account(Balances);

impl Default for Account {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

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
            store: Default::default(),
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
        state.hash = state.compute_hash();
        state
    }

    /// Get the current balances for all currency denominations for the
    /// account with the given ID.
    ///
    /// Returns `None` if no such account exists.
    pub fn get_balances(&self, account_id: &str) -> Option<Balances> {
        self.store.0.get(account_id).map(|acc| acc.0.clone())
    }

    /// Attempts to transfer the given quantity of funds (in the specified
    /// denomination) from the source to the destination account.
    pub fn transfer(
        &mut self,
        src_account_id: &str,
        dest_account_id: &str,
        amounts: Vec<Coin>,
    ) -> Result<()> {
        // Extract the u64 value associated with coin amounts. Unfortunately
        // right now the only way to do this is to print the amount to a
        // string and then parse it back.
        // TODO(thane): Add conversion functionality to u64 to `cosmos-sdk-rs`.
        let amounts = amounts
            .iter()
            .map(|coin| {
                let amt = coin.amount.to_string();
                match u64::from_str(&amt) {
                    Ok(amt) => Ok((amt, coin.denom.to_string())),
                    Err(e) => Err(flex_error::DefaultTracer::from(
                        StateError::CannotParseCoinAmount(amt, coin.denom.to_string()),
                    )
                    .add_message(&e)),
                }
            })
            .collect::<Result<Vec<(u64, String)>>>()?;

        let mut src_balances = match self.store.get(src_account_id) {
            Some(sb) => sb.0.clone(),
            None => return Err(StateError::NoSuchAccount(src_account_id.to_owned()).into()),
        };
        let mut dest_balances = self
            .store
            .get(dest_account_id)
            .map(|acc| acc.0.clone())
            .unwrap_or_else(HashMap::new);

        for (amount, denom) in amounts {
            let mut src_balance = match src_balances.get(&denom) {
                Some(b) => *b,
                None => return Err(StateError::InsufficientSourceFunds.into()),
            };
            if src_balance < amount {
                return Err(StateError::InsufficientSourceFunds.into());
            }
            let mut dest_balance = dest_balances.get(&denom).map(Clone::clone).unwrap_or(0_u64);
            if dest_balance > u64::MAX - amount {
                return Err(StateError::DestFundOverflow.into());
            }
            src_balance -= amount;
            dest_balance += amount;
            src_balances.insert(denom.to_owned(), src_balance);
            dest_balances.insert(denom.to_owned(), dest_balance);
        }

        // Store the updated account balances
        self.store
            .insert(src_account_id.to_owned(), Account(src_balances));
        self.store
            .insert(dest_account_id.to_owned(), Account(dest_balances));
        Ok(())
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
        self.hash = self.compute_hash();
        (self.height, self.hash())
    }

    fn compute_hash(&self) -> Vec<u8> {
        let mut hash = BytesMut::with_capacity(APP_HASH_LENGTH);
        encode_varint(self.store.len() as u64, &mut hash);
        hash.to_vec()
    }
}
