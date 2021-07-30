use crate::app::modules::{Error as ModuleError, IdentifiableBy, Module};
use crate::app::store::memory::Memory;
use crate::app::store::{Height, Store};
use cosmos_sdk::bank::MsgSend;
use cosmos_sdk::proto;
use prost::{DecodeError, Message};
use prost_types::Any;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::str::FromStr;
use tendermint_proto::abci::Event;

/// Unique identifiers for accounts.
pub type AccountId = String;

/// A currency denomination.
pub type Denom = String;

/// A mapping of currency denomination identifiers to balances.
#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
pub struct Balances(HashMap<Denom, u64>);

/// A single account's details.
// pub struct Account(HashMap<AccountId, Balances>);

pub struct Bank;

pub enum Error {
    MsgDecodeErr(DecodeError),
    MsgValidationErr(String),
}

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::BankError(e)
    }
}

impl Module<Memory> for Bank {
    fn decode<T: Message + Default>(message: Any) -> Result<T, ModuleError> {
        if message.type_url != "/cosmos.bank.v1beta1.MsgSend" {
            return Err(ModuleError::Unhandled);
        }
        Message::decode(message.value.as_ref()).map_err(|e| Error::MsgDecodeErr(e).into())
    }

    fn deliver(store: &mut Memory, message: Any) -> Result<Vec<Event>, ModuleError> {
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

        let path = format!("{}/accounts/{}", Self::identifier(), message.from_address)
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
}
