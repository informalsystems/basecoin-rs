use crate::app::modules::{Error as ModuleError, Module, QueryResult};
use crate::app::store::{
    Codec, Height, JsonCodec, JsonStore, Path, ProvableStore, SharedStore, Store, SubStore,
};

use std::collections::HashMap;
use std::convert::TryInto;
use std::num::ParseIntError;
use std::str::FromStr;

use cosmrs::bank::MsgSend;
use cosmrs::{proto, AccountId};
use flex_error::{define_error, TraceError};
use prost::{DecodeError, Message};
use prost_types::Any;
use serde::{Deserialize, Serialize};
use tendermint_proto::abci::Event;
use tracing::{debug, trace};

/// A currency denomination.
pub type Denom = String;

define_error! {
    #[derive(Eq, PartialEq)]
    Error {
        MsgDecodeFailure
            [ TraceError<DecodeError> ]
            | _ | { "failed to decode message" },
        MsgValidationFailure
            { reason: String }
            | e | { format!("failed to validate message: {}", e.reason) },
        NonExistentAccount
            { account: AccountId }
            | e | { format!("account {} doesn't exist", e.account) },
        InvalidAmount
            [ TraceError<ParseIntError> ]
            | _ | { "invalid amount specified" },
        InsufficientSourceFunds
            | _ | { "insufficient funds in sender account" },
        DestFundOverflow
            | _ | { "receiver account funds overflow" },
        Store
            { reason: String }
            | e | { format!("Store error: {}", e.reason) },
    }
}

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::bank(e)
    }
}

/// A mapping of currency denomination identifiers to balances.
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(transparent)]
pub struct Balances(HashMap<Denom, u64>);

#[derive(Clone)]
struct AccountsPath(AccountId);

impl From<AccountsPath> for Path {
    fn from(path: AccountsPath) -> Self {
        format!("accounts/{}", path.0).try_into().unwrap() // safety - cannot fail as AccountsPath is correct-by-construction
    }
}

/// The bank module
pub struct Bank<S> {
    /// Handle to store instance
    /// The module is guaranteed exclusive access to all paths in the store key-space.
    store: SharedStore<S>,
    /// A typed-store for accounts
    account_store: JsonStore<SharedStore<S>, AccountsPath, Balances>,
}

impl<S: ProvableStore + Default> Bank<SubStore<S>> {
    pub fn new(store: SubStore<S>) -> Self {
        let store = SharedStore::new(store);
        Self {
            store: store.clone(),
            account_store: SubStore::typed_store(store),
        }
    }
}

impl<S: Store> Bank<S> {
    fn decode<T: Message + Default>(message: Any) -> Result<T, ModuleError> {
        if message.type_url != "/cosmos.bank.v1beta1.MsgSend" {
            return Err(ModuleError::not_handled());
        }
        Message::decode(message.value.as_ref()).map_err(|e| Error::msg_decode_failure(e).into())
    }
}

impl<S: ProvableStore> Module<S> for Bank<S> {
    fn deliver(&mut self, message: Any) -> Result<Vec<Event>, ModuleError> {
        let message: MsgSend = Self::decode::<proto::cosmos::bank::v1beta1::MsgSend>(message)?
            .try_into()
            .map_err(|e| Error::msg_validation_failure(format!("{:?}", e)))?;

        let amounts = message
            .amount
            .iter()
            .map(|coin| {
                let amt = coin.amount.to_string();
                match u64::from_str(&amt) {
                    Ok(amt) => Ok((amt, coin.denom.to_string())),
                    Err(e) => Err(Error::invalid_amount(e)),
                }
            })
            .collect::<Result<Vec<(u64, String)>, Error>>()?;

        let src_path = AccountsPath(message.from_address.clone());
        let mut src_balances: Balances = match self.account_store.get(Height::Pending, &src_path) {
            Some(sb) => sb,
            None => {
                return Err(Error::non_existent_account(message.from_address).into());
            }
        };

        let dst_path = AccountsPath(message.to_address);
        let mut dst_balances: Balances = self
            .account_store
            .get(Height::Pending, &dst_path)
            .unwrap_or_default();

        // TODO(hu55a1n1): extract account related code into the `auth` module
        for (amount, denom) in amounts {
            let mut src_balance = match src_balances.0.get(&denom) {
                Some(b) if *b >= amount => *b,
                _ => return Err(Error::insufficient_source_funds().into()),
            };
            let mut dst_balance = dst_balances
                .0
                .get(&denom)
                .map(Clone::clone)
                .unwrap_or(0_u64);
            if dst_balance > u64::MAX - amount {
                return Err(Error::dest_fund_overflow().into());
            }
            src_balance -= amount;
            dst_balance += amount;
            src_balances.0.insert(denom.to_owned(), src_balance);
            dst_balances.0.insert(denom.to_owned(), dst_balance);
        }

        // Store the updated account balances
        self.account_store
            .set(src_path, src_balances)
            .map_err(|e| Error::store(format!("{:?}", e)))?;
        self.account_store
            .set(dst_path, dst_balances)
            .map_err(|e| Error::store(format!("{:?}", e)))?;

        Ok(vec![])
    }

    fn init(&mut self, app_state: serde_json::Value) {
        debug!("Initializing bank module");

        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let accounts: HashMap<String, Balances> = serde_json::from_value(app_state).unwrap();
        for account in accounts {
            trace!("Adding account ({}) => {:?}", account.0, account.1);

            let account_id = AccountId::from_str(&account.0).unwrap();
            self.account_store
                .set(AccountsPath(account_id), account.1)
                .unwrap();
        }
    }

    fn query(
        &self,
        data: &[u8],
        _path: Option<&Path>,
        height: Height,
        _prove: bool,
    ) -> Result<QueryResult, ModuleError> {
        let account_id = match String::from_utf8(data.to_vec()) {
            Ok(s) if s.starts_with("cosmos") => s, // TODO(hu55a1n1): check if valid identifier
            _ => return Err(ModuleError::not_handled()),
        };

        let account_id =
            AccountId::from_str(&account_id).map_err(|_| ModuleError::not_handled())?;

        trace!("Attempting to get account ID: {}", account_id);

        match self
            .account_store
            .get(height, &AccountsPath(account_id.clone()))
        {
            None => Err(Error::non_existent_account(account_id).into()),
            Some(balance) => Ok(QueryResult {
                data: JsonCodec::encode(&balance).unwrap().into_bytes(),
                proof: None,
            }),
        }
    }

    fn commit(&mut self) -> Result<Vec<u8>, S::Error> {
        self.store.commit()
    }

    fn store(&self) -> SharedStore<S> {
        self.store.clone()
    }
}
