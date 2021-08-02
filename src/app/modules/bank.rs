use crate::app::modules::{Error as ModuleError, IdentifiableBy, Module};
use crate::app::store::memory::Memory;
use crate::app::store::{Height, Path, Store};
use cosmos_sdk::bank::MsgSend;
use cosmos_sdk::proto;
use prost::{DecodeError, Message};
use prost_types::Any;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::num::ParseIntError;
use std::str::FromStr;
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

pub struct Bank;

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

impl Bank {
    fn decode<T: Message + Default>(message: Any) -> Result<T, ModuleError> {
        if message.type_url != "/cosmos.bank.v1beta1.MsgSend" {
            return Err(ModuleError::Unhandled);
        }
        Message::decode(message.value.as_ref()).map_err(|e| Error::MsgDecodeErr(e).into())
    }
}

impl Module<Memory> for Bank {
    fn deliver(&self, store: &mut Memory, message: Any) -> Result<Vec<Event>, ModuleError> {
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
                    Err(e) => return Err(Error::AmountParseErr(e).into()),
                }
            })
            .collect::<Result<Vec<(u64, String)>, Error>>()?;

        let src_path = format!("{}/accounts/{}", self.identifier(), message.from_address)
            .try_into()
            .unwrap();
        let mut src_balances: Balances = match store.get(Height::Pending, &src_path) {
            Some(sb) => serde_json::from_str(&String::from_utf8(sb).unwrap()).unwrap(),
            None => return Err(Error::NonExistentAccount(message.from_address.to_string()).into()),
        };

        let dst_path = format!("{}/accounts/{}", self.identifier(), message.to_address)
            .try_into()
            .unwrap();
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

    fn init(&self, store: &mut Memory, app_state: serde_json::Value) {
        debug!("Initializing bank module");

        let accounts: HashMap<AccountId, Balances> = serde_json::from_value(app_state).unwrap();
        for account in accounts {
            let path = format!("{}/accounts/{}", self.identifier(), account.0)
                .try_into()
                .unwrap();
            store
                .set(&path, serde_json::to_string(&account.1).unwrap().into())
                .unwrap();

            debug!("Added account ({}) => {:?}", account.0, account.1);
        }
    }

    fn query(
        &self,
        store: &Memory,
        data: &[u8],
        _path: &Path,
        height: Height,
    ) -> Result<Vec<u8>, ModuleError> {
        let account_id = match String::from_utf8(data.to_vec()) {
            Ok(s) => s,
            Err(e) => panic!("Failed to interpret key as UTF-8: {}", e),
        };
        debug!("Attempting to get account ID: {}", account_id);

        let path = format!("{}/accounts/{}", self.identifier(), account_id)
            .try_into()
            .unwrap();

        match store.get(height, &path) {
            None => Err(Error::NonExistentAccount(account_id).into()),
            Some(balance) => Ok(balance),
        }
    }
}
