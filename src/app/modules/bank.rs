use crate::app::modules::{Error as ModuleError, Module};
use crate::app::store::{Height, Path, PrefixedPath, Store};
use cosmos_sdk::bank::MsgSend;
use cosmos_sdk::proto;
use prost::{DecodeError, Message};
use prost_types::Any;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::num::ParseIntError;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tendermint_proto::abci::Event;
use tracing::debug;

/// Unique identifiers for accounts.
pub type AccountId = String;

/// A currency denomination.
pub type Denom = String;

/// A mapping of currency denomination identifiers to balances.
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(transparent)]
pub struct Balances(HashMap<Denom, u64>);

pub struct Bank<S: Store> {
    pub store: Arc<RwLock<S>>,
}

#[derive(Debug)]
pub enum Error {
    MsgDecodeErr(DecodeError),
    MsgValidationErr(String),
    NonExistentAccount(AccountId),
    AmountParseErr(ParseIntError),
    InsufficientSourceFunds,
    DestFundOverflow,
}

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::BankError(e)
    }
}

impl<S: Store> Bank<S> {
    fn decode<T: Message + Default>(message: Any) -> Result<T, ModuleError> {
        if message.type_url != "/cosmos.bank.v1beta1.MsgSend" {
            return Err(ModuleError::Unhandled);
        }
        Message::decode(message.value.as_ref()).map_err(|e| Error::MsgDecodeErr(e).into())
    }
}

impl<S: Store> Module for Bank<S> {
    fn deliver(&mut self, message: Any) -> Result<Vec<Event>, ModuleError> {
        let message: MsgSend = Self::decode::<proto::cosmos::bank::v1beta1::MsgSend>(message)?
            .try_into()
            .map_err(|e| Error::MsgValidationErr(format!("{:?}", e)))?;

        let amounts = message
            .amount
            .iter()
            .map(|coin| {
                let amt = coin.amount.to_string();
                match u64::from_str(&amt) {
                    Ok(amt) => Ok((amt, coin.denom.to_string())),
                    Err(e) => Err(Error::AmountParseErr(e)),
                }
            })
            .collect::<Result<Vec<(u64, String)>, Error>>()?;

        let mut store = self.store.write().unwrap();
        let src_path = self.prefixed_path(&format!("accounts/{}", message.from_address));
        let mut src_balances: Balances = match store.get(Height::Pending, &src_path) {
            Some(sb) => serde_json::from_str(&String::from_utf8(sb).unwrap()).unwrap(),
            None => return Err(Error::NonExistentAccount(message.from_address.to_string()).into()),
        };

        let dst_path = self.prefixed_path(&format!("accounts/{}", message.to_address));
        let mut dst_balances: Balances = store
            .get(Height::Pending, &dst_path)
            .map(|db| serde_json::from_str(&String::from_utf8(db).unwrap()).unwrap())
            .unwrap_or_else(Default::default);

        for (amount, denom) in amounts {
            let mut src_balance = match src_balances.0.get(&denom) {
                Some(b) if *b >= amount => *b,
                _ => return Err(Error::InsufficientSourceFunds.into()),
            };
            let mut dst_balance = dst_balances
                .0
                .get(&denom)
                .map(Clone::clone)
                .unwrap_or(0_u64);
            if dst_balance > u64::MAX - amount {
                return Err(Error::DestFundOverflow.into());
            }
            src_balance -= amount;
            dst_balance += amount;
            src_balances.0.insert(denom.to_owned(), src_balance);
            dst_balances.0.insert(denom.to_owned(), dst_balance);
        }

        // Store the updated account balances
        store
            .set(
                &src_path,
                serde_json::to_string(&src_balances).unwrap().into(),
            )
            .unwrap();
        store
            .set(
                &dst_path,
                serde_json::to_string(&dst_balances).unwrap().into(),
            )
            .unwrap();

        Ok(vec![])
    }

    fn init(&mut self, app_state: serde_json::Value) {
        debug!("Initializing bank module");

        let mut store = self.store.write().unwrap();
        let accounts: HashMap<AccountId, Balances> = serde_json::from_value(app_state).unwrap();
        for account in accounts {
            let path = self.prefixed_path(&format!("accounts/{}", account.0));
            store
                .set(&path, serde_json::to_string(&account.1).unwrap().into())
                .unwrap();

            debug!("Added account ({}) => {:?}", account.0, account.1);
        }
    }

    fn query(&self, data: &[u8], _path: &Path, height: Height) -> Result<Vec<u8>, ModuleError> {
        let account_id = match String::from_utf8(data.to_vec()) {
            Ok(s) => s,
            Err(e) => panic!("Failed to interpret key as UTF-8: {}", e),
        };
        debug!("Attempting to get account ID: {}", account_id);

        let store = self.store.read().unwrap();
        let path = self.prefixed_path(&format!("accounts/{}", account_id));
        match store.get(height, &path) {
            None => Err(Error::NonExistentAccount(account_id).into()),
            Some(balance) => Ok(balance),
        }
    }
}
