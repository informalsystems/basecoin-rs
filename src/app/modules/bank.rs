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
use std::str::FromStr;
use tendermint_proto::abci::Event;
use tracing::debug;

/// Unique identifiers for accounts.
pub type AccountId = String;

/// A currency denomination.
pub type Denom = String;

/// A mapping of currency denomination identifiers to balances.
#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
pub struct Balances(HashMap<Denom, u64>);

pub struct Bank;

#[derive(Debug)]
pub enum Error {
    MsgDecodeErr(DecodeError),
    MsgValidationErr(String),
    // SerdeError(serde_json::Error),
    NonExistentAccount,
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
        if let Err(e) = u64::from_str(message.amount[0].amount.to_string().as_str()) {
            return Err(Error::MsgValidationErr(format!(
                "failed to decode amount: {}",
                e.to_string()
            ))
            .into());
        }

        let mut balances = HashMap::new();
        balances.insert("basecoin".to_owned(), 100u64);

        let path = format!("{}/accounts/{}", self.identifier(), message.from_address)
            .try_into()
            .unwrap();
        store
            .set(&path, serde_json::to_string(&balances).unwrap().into())
            .unwrap();

        let balances = store.get(Height::Pending, &path).expect("acc not found");
        let balances: Balances =
            serde_json::from_str(&String::from_utf8(balances).unwrap()).unwrap();
        println!("{:?}", balances);

        Ok(vec![])
    }

    fn init(&self, store: &mut Memory, app_state: serde_json::Value) {
        let accounts: HashMap<AccountId, Balances> = serde_json::from_value(app_state).unwrap();

        for account in accounts {
            let path = format!("{}/accounts/{}", self.identifier(), account.0)
                .try_into()
                .unwrap();
            store
                .set(&path, serde_json::to_string(&account.1).unwrap().into())
                .unwrap();
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
            None => Err(Error::NonExistentAccount.into()),
            Some(balance) => Ok(balance),
        }
    }
}
