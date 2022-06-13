use crate::app::modules::auth::{AccountKeeper, AccountReader, AuthAccount, ACCOUNT_PREFIX};
use crate::app::modules::{Error as ModuleError, Module, QueryResult};
use crate::app::store::{
    Codec, Height, JsonCodec, JsonStore, Path, ProvableStore, SharedStore, Store, TypedStore,
};

use std::collections::HashMap;
use std::convert::TryInto;
use std::num::ParseIntError;
use std::str::FromStr;

use cosmrs::bank::MsgSend;
use cosmrs::{proto, AccountId, Coin as MsgCoin};
use flex_error::{define_error, TraceError};
use ibc_proto::google::protobuf::Any;
use prost::{DecodeError, Message};
use serde::{Deserialize, Serialize};
use tendermint_proto::abci::Event;
use tracing::{debug, trace};

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

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone, Hash, Eq)]
#[serde(transparent)]
pub struct Denom(String);

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Coin {
    pub denom: Denom,
    pub amount: u64,
}

impl From<(Denom, u64)> for Coin {
    fn from((denom, amount): (Denom, u64)) -> Self {
        Self { denom, amount }
    }
}

impl From<&MsgCoin> for Coin {
    fn from(coin: &MsgCoin) -> Self {
        Self {
            denom: Denom(coin.denom.to_string()),
            amount: coin.amount.to_string().parse().unwrap(),
        }
    }
}

/// A mapping of currency denomination identifiers to balances.
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(transparent)]
pub struct Balances(Vec<Coin>);

#[derive(Clone)]
struct BalancesPath(AccountId);

impl From<BalancesPath> for Path {
    fn from(path: BalancesPath) -> Self {
        format!("balances/{}", path.0).try_into().unwrap() // safety - cannot fail as AccountsPath is correct-by-construction
    }
}

pub trait BankReader {
    type Address;
    type Denom;
    type Coin;
    type Coins: IntoIterator<Item = Self::Coin>;

    fn get_all_balances_at_height(&self, height: Height, address: Self::Address) -> Self::Coins;

    fn get_all_balances(&self, address: Self::Address) -> Self::Coins {
        self.get_all_balances_at_height(Height::Pending, address)
    }
}

pub trait BankKeeper {
    type Error;
    type Address;
    type Denom;
    type Coin;
    type Coins: IntoIterator<Item = Self::Coin>;

    fn set_balances(
        &mut self,
        address: Self::Address,
        coins: Self::Coins,
    ) -> Result<(), Self::Error>;
}

pub struct BankBalanceReader<S> {
    balance_store: JsonStore<SharedStore<S>, BalancesPath, Balances>,
}

impl<S: Store> BankReader for BankBalanceReader<S> {
    type Address = AccountId;
    type Denom = Denom;
    type Coin = Coin;
    type Coins = Vec<Coin>;

    fn get_all_balances_at_height(&self, height: Height, address: Self::Address) -> Self::Coins {
        self.balance_store
            .get(height, &BalancesPath(address))
            .map(|b| b.0)
            .unwrap_or_default()
    }
}

pub struct BankBalanceKeeper<S> {
    balance_store: JsonStore<SharedStore<S>, BalancesPath, Balances>,
}

impl<S: Store> BankKeeper for BankBalanceKeeper<S> {
    type Error = ();
    type Address = AccountId;
    type Denom = Denom;
    type Coin = Coin;
    type Coins = Vec<Self::Coin>;

    fn set_balances(
        &mut self,
        address: Self::Address,
        coins: Self::Coins,
    ) -> Result<(), Self::Error> {
        self.balance_store
            .set(BalancesPath(address), Balances(coins))
            .map(|_| ())
            .map_err(|_| ())
    }
}

/// The bank module
pub struct Bank<S, AR, AK> {
    /// Handle to store instance
    /// The module is guaranteed exclusive access to all paths in the store key-space.
    store: SharedStore<S>,
    balance_reader: BankBalanceReader<S>,
    balance_keeper: BankBalanceKeeper<S>,
    account_reader: AR,
    #[allow(dead_code)]
    account_keeper: AK,
}

impl<S: ProvableStore + Default, AR: AccountReader, AK: AccountKeeper> Bank<S, AR, AK> {
    pub fn new(store: SharedStore<S>, account_reader: AR, account_keeper: AK) -> Self {
        Self {
            store: store.clone(),
            balance_reader: BankBalanceReader {
                balance_store: TypedStore::new(store.clone()),
            },
            balance_keeper: BankBalanceKeeper {
                balance_store: TypedStore::new(store),
            },
            account_reader,
            account_keeper,
        }
    }
}

impl<S: Store, AR: AccountReader, AK: AccountKeeper> Bank<S, AR, AK> {
    fn decode<T: Message + Default>(message: Any) -> Result<T, ModuleError> {
        if message.type_url != "/cosmos.bank.v1beta1.MsgSend" {
            return Err(ModuleError::not_handled());
        }
        Message::decode(message.value.as_ref()).map_err(|e| Error::msg_decode_failure(e).into())
    }
}

impl<S: ProvableStore, AR: AccountReader + Send + Sync, AK: AccountKeeper + Send + Sync> Module
    for Bank<S, AR, AK>
where
    <AR as AccountReader>::Address: From<cosmrs::AccountId>,
    <AK as AccountKeeper>::Account: From<AuthAccount>,
{
    type Store = S;

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, ModuleError> {
        let message: MsgSend = Self::decode::<proto::cosmos::bank::v1beta1::MsgSend>(message)?
            .try_into()
            .map_err(|e| Error::msg_validation_failure(format!("{:?}", e)))?;

        let _ = self
            .account_reader
            .get_account(message.from_address.clone().into())
            .map_err(|_| Error::non_existent_account(message.from_address.clone()))?;

        // Note: we allow transfers to non-existent destination accounts

        let mut src_balances = self
            .balance_reader
            .get_all_balances(message.from_address.clone());
        let mut dst_balances = self
            .balance_reader
            .get_all_balances(message.to_address.clone());

        let amounts: Vec<Coin> = message.amount.iter().map(|amt| amt.into()).collect();
        for Coin { denom, amount } in amounts {
            let mut src_balance = src_balances
                .iter_mut()
                .find(|c| c.denom == denom)
                .ok_or_else(Error::insufficient_source_funds)?;

            if dst_balances.iter().any(|c| c.denom == denom) {
                dst_balances.push(Coin {
                    denom: denom.clone(),
                    amount: 0u64,
                });
            }

            let mut dst_balance = dst_balances.iter_mut().find(|c| c.denom == denom).unwrap();

            if dst_balance.amount > u64::MAX - amount {
                return Err(Error::dest_fund_overflow().into());
            }

            src_balance.amount -= amount;
            dst_balance.amount += amount;
        }

        // Store the updated account balances
        self.balance_keeper
            .set_balances(message.from_address, src_balances)
            .map_err(|e| Error::store(format!("{:?}", e)))?;
        self.balance_keeper
            .set_balances(message.to_address, dst_balances)
            .map_err(|e| Error::store(format!("{:?}", e)))?;

        Ok(vec![])
    }

    fn init(&mut self, app_state: serde_json::Value) {
        debug!("Initializing bank module");

        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let accounts: HashMap<String, HashMap<Denom, u64>> =
            serde_json::from_value(app_state).unwrap();
        for (account, balances) in accounts {
            trace!("Adding account ({}) => {:?}", account, balances);

            let account_id = AccountId::from_str(&account).unwrap();
            self.balance_keeper
                .set_balances(account_id, balances.into_iter().map(|b| b.into()).collect())
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
            Ok(s) if s.starts_with(ACCOUNT_PREFIX) => s, // TODO(hu55a1n1): check if valid identifier
            _ => return Err(ModuleError::not_handled()),
        };

        let account_id =
            AccountId::from_str(&account_id).map_err(|_| ModuleError::not_handled())?;

        trace!("Attempting to get account ID: {}", account_id);

        let _ = self
            .account_reader
            .get_account(account_id.clone().into())
            .map_err(|_| Error::non_existent_account(account_id.clone()))?;

        let balance = self
            .balance_reader
            .get_all_balances_at_height(height, account_id);

        Ok(QueryResult {
            data: JsonCodec::encode(&balance).unwrap().into_bytes(),
            proof: None,
        })
    }

    fn store_mut(&mut self) -> &mut SharedStore<S> {
        &mut self.store
    }

    fn store(&self) -> &SharedStore<S> {
        &self.store
    }
}
