use crate::app::modules::{Error as ModuleError, Module};
use crate::app::store::{Height, Path, Store};

use std::collections::HashMap;
use std::convert::TryInto;
use std::num::ParseIntError;
use std::str::FromStr;

use cosmos_sdk::bank::MsgSend;
use cosmos_sdk::proto;
use flex_error::{define_error, TraceError};
use prost::{DecodeError, Message};
use prost_types::Any;
use serde::{Deserialize, Serialize};
use tendermint_proto::abci::Event;
use tracing::debug;

/// Unique identifiers for accounts.
pub type AccountId = String;

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

pub struct Bank<S> {
    pub store: S,
}

impl<S: Store> Bank<S> {
    fn decode<T: Message + Default>(message: Any) -> Result<T, ModuleError> {
        if message.type_url != "/cosmos.bank.v1beta1.MsgSend" {
            return Err(ModuleError::unhandled());
        }
        Message::decode(message.value.as_ref()).map_err(|e| Error::msg_decode_failure(e).into())
    }
}

impl<S: Store> Module for Bank<S> {
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

        let src_path = format!("accounts/{}", message.from_address)
            .try_into()
            .unwrap();
        let mut src_balances: Balances = match self.store.get(Height::Pending, &src_path) {
            Some(sb) => serde_json::from_str(&String::from_utf8(sb).unwrap()).unwrap(),
            None => {
                return Err(Error::non_existent_account(message.from_address.to_string()).into())
            }
        };

        let dst_path = format!("accounts/{}", message.to_address)
            .try_into()
            .unwrap();
        let mut dst_balances: Balances = self
            .store
            .get(Height::Pending, &dst_path)
            .map(|db| serde_json::from_str(&String::from_utf8(db).unwrap()).unwrap())
            .unwrap_or_else(Default::default);

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
        self.store
            .set(
                &src_path,
                serde_json::to_string(&src_balances).unwrap().into(),
            )
            .unwrap();
        self.store
            .set(
                &dst_path,
                serde_json::to_string(&dst_balances).unwrap().into(),
            )
            .unwrap();

        Ok(vec![])
    }

    fn init(&mut self, app_state: serde_json::Value) {
        debug!("Initializing bank module");

        let accounts: HashMap<AccountId, Balances> = serde_json::from_value(app_state).unwrap();
        for account in accounts {
            let path = format!("accounts/{}", account.0).try_into().unwrap();
            self.store
                .set(&path, serde_json::to_string(&account.1).unwrap().into())
                .unwrap();

            debug!("Added account ({}) => {:?}", account.0, account.1);
        }
    }

    fn query(&self, data: &[u8], _path: &Path, height: Height) -> Result<Vec<u8>, ModuleError> {
        let account_id = match String::from_utf8(data.to_vec()) {
            Ok(s) if s.starts_with("cosmos") => s,
            _ => return Err(ModuleError::unhandled()),
        };

        debug!("Attempting to get account ID: {}", account_id);

        let path = format!("accounts/{}", account_id).try_into().unwrap();
        match self.store.get(height, &path) {
            None => Err(Error::non_existent_account(account_id).into()),
            Some(balance) => Ok(balance),
        }
    }
}