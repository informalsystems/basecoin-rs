//! The state of the basecoin ABCI application.

use crate::app::ibc::Context;
use crate::encoding::encode_varint;
use crate::result::Result;
use bytes::BytesMut;
use cosmos_sdk::Coin;
use flex_error::ErrorMessageTracer;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use thiserror::Error;
use tracing::debug;

const APP_HASH_LENGTH: usize = 16;

/// Unique identifiers for accounts.
pub type AccountId = String;

/// A currency denomination.
pub type Denom = String;

/// A mapping of currency denomination identifiers to balances.
pub type Balances = HashMap<Denom, u64>;

/// Our account store.
#[derive(Debug, Clone, Deserialize)]
pub struct Store {
    accounts: HashMap<AccountId, Account>,
    pub context: Context,
}

impl Store {
    fn get(&self, account_id: &str) -> Option<&Account> {
        self.accounts.get(account_id)
    }

    fn insert(&mut self, account_id: AccountId, account: Account) {
        self.accounts.insert(account_id, account);
    }

    fn len(&self) -> usize {
        self.accounts.len()
    }
}

impl Default for Store {
    fn default() -> Self {
        Self {
            accounts: HashMap::new(),
            context: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq)]
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
    pub store: Store,

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
    /// Override all balances in the store (useful for initialization).
    pub fn set_balances(&mut self, store: Store) {
        self.store = store;
        self.hash = self.compute_hash();
    }

    /// Get the current balances for all currency denominations for the
    /// account with the given ID.
    ///
    /// Returns `None` if no such account exists.
    pub fn get_balances(&self, account_id: &str) -> Option<Balances> {
        self.store.accounts.get(account_id).map(|acc| acc.0.clone())
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
                Some(b) if *b >= amount => *b,
                _ => return Err(StateError::InsufficientSourceFunds.into()),
            };
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
        debug!("New account balances: {:?}", self.store);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_value, json};

    #[inline]
    fn create_coin(denom: &str, amount: u64) -> Result<Coin> {
        use cosmos_sdk::{Decimal, Denom};
        Ok(Coin {
            denom: Denom::from_str(denom)?,
            amount: Decimal::from(amount),
        })
    }

    #[test]
    fn test_transfer() -> Result<()> {
        let app_state = json!({
          "cosmos1snd5m4h0wt5ur55d47vpxla389r2xkf8dl6g9w": { "basecoin" : 1000, "othercoin" : 1000 },
          "cosmos1t2e0nyjhwn3revunvf2uperhftvhzu4euuzva9": { "basecoin" : 250, "othercoin" : 5000 },
          "cosmos1uawm90a5xm36kjmaazv89nxmfr8s8cyzkjqytd": { "acidcoin": 500 },
          "cosmos1ny9epydqnr7ymqhmgfvlshp3485cuqlmt7vsmf": { },
          "cosmos1xwgdxu4ahd9eevtfnq5f7w4td3rqnph4llnngw": { "acidcoin": 500, "basecoin" : 0, "othercoin": 100 },
          "cosmos1mac8xqhun2c3y0njptdmmh3vy8nfjmtm6vua9u": { "basecoin" : 1000 },
          "cosmos1wkvwnez6fkjn63xaz7nzpm4zxcd9cetqmyh2y8": { "basecoin" : 1 },
          "cosmos166vcha998g7tl8j8cq0kwa8rfvm68cqmj88cff": { "basecoin" : u64::MAX }
        });
        let mut state = BaseCoinState {
            store: from_value(app_state)?,
            ..Default::default()
        };

        // single denom transfer
        let from = "cosmos1snd5m4h0wt5ur55d47vpxla389r2xkf8dl6g9w";
        let to = "cosmos1t2e0nyjhwn3revunvf2uperhftvhzu4euuzva9";
        let denom_basecoin = "basecoin";
        assert!(state
            .transfer(from, to, vec![create_coin(denom_basecoin, 100)?])
            .is_ok());
        assert_eq!(state.get_balances(from).unwrap()[denom_basecoin], 900);
        assert_eq!(state.get_balances(to).unwrap()[denom_basecoin], 350);

        // insufficient source funds
        assert_eq!(
            state
                .transfer(from, to, vec![create_coin(denom_basecoin, 901)?])
                .err()
                .unwrap()
                .downcast::<StateError>()?,
            StateError::InsufficientSourceFunds
        );

        // unknown denom transfer
        assert_eq!(
            state
                .transfer(from, to, vec![create_coin("unknowncoin", 10)?])
                .err()
                .unwrap()
                .downcast::<StateError>()?,
            StateError::InsufficientSourceFunds
        );

        // transfer from non-existent account
        let non_existent_acc = "cosmos1awx9pmmr82p7097wx6gft923k4ugwd5vxuhf3s";
        assert_eq!(
            state
                .transfer(non_existent_acc, to, vec![create_coin(denom_basecoin, 10)?])
                .err()
                .unwrap()
                .downcast::<StateError>()?,
            StateError::NoSuchAccount(non_existent_acc.to_owned())
        );

        // transfer to non-existent account
        assert!(state
            .transfer(
                from,
                non_existent_acc,
                vec![create_coin(denom_basecoin, 100)?],
            )
            .is_ok());
        assert_eq!(state.get_balances(from).unwrap()[denom_basecoin], 800);
        assert_eq!(
            state.get_balances(non_existent_acc).unwrap()[denom_basecoin],
            100
        );

        // transfer from newly created account
        let existent_acc = non_existent_acc;
        assert!(state
            .transfer(existent_acc, to, vec![create_coin(denom_basecoin, 100)?])
            .is_ok());
        assert_eq!(state.get_balances(existent_acc).unwrap()[denom_basecoin], 0);
        assert_eq!(state.get_balances(to).unwrap()[denom_basecoin], 450);

        // overflow
        let pauper = "cosmos1wkvwnez6fkjn63xaz7nzpm4zxcd9cetqmyh2y8";
        let ultra_high_net_worth_individual = "cosmos166vcha998g7tl8j8cq0kwa8rfvm68cqmj88cff";
        assert_eq!(
            state
                .transfer(
                    pauper,
                    ultra_high_net_worth_individual,
                    vec![create_coin(denom_basecoin, 1)?],
                )
                .err()
                .unwrap()
                .downcast::<StateError>()?,
            StateError::DestFundOverflow
        );

        // multi-coin transfer
        let denom_othercoin = "othercoin";
        assert!(state
            .transfer(
                from,
                to,
                vec![
                    create_coin(denom_basecoin, 100)?,
                    create_coin(denom_othercoin, 100)?
                ],
            )
            .is_ok());
        assert_eq!(state.get_balances(from).unwrap()[denom_basecoin], 700);
        assert_eq!(state.get_balances(to).unwrap()[denom_basecoin], 550);
        assert_eq!(state.get_balances(from).unwrap()[denom_othercoin], 900);
        assert_eq!(state.get_balances(to).unwrap()[denom_othercoin], 5100);

        // multi-coin transfer with failure
        assert_eq!(
            state
                .transfer(
                    from,
                    to,
                    vec![
                        create_coin(denom_basecoin, 10)?,
                        create_coin("unknowncoin", 10)?
                    ],
                )
                .err()
                .unwrap()
                .downcast::<StateError>()?,
            StateError::InsufficientSourceFunds
        );
        assert_eq!(state.get_balances(from).unwrap()[denom_basecoin], 700);
        assert_eq!(state.get_balances(to).unwrap()[denom_basecoin], 550);

        Ok(())
    }

    // TODO(hu55a1n1): Add quickcheck test that checks equality of pre/post state after transfer
}
