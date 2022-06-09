use crate::app::modules::auth::{AccountKeeper, AccountReader, AuthAccount};
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
use ibc::bigint::U256;
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
pub struct Denom(pub String);

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Coin {
    pub denom: Denom,
    pub amount: U256,
}

impl From<(Denom, U256)> for Coin {
    fn from((denom, amount): (Denom, U256)) -> Self {
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
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
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

    /// This function should enable sending ibc fungible tokens from one account to another
    fn send_coins(
        &mut self,
        from: Self::Address,
        to: Self::Address,
        amount: Self::Coins,
    ) -> Result<(), Self::Error>;

    /// This function to enable minting ibc tokens to a user account
    fn mint_coins(
        &mut self,
        account: Self::Address,
        amount: Self::Coins,
    ) -> Result<(), Self::Error>;

    /// This function should enable burning of minted tokens in a user account
    fn burn_coins(
        &mut self,
        account: Self::Address,
        amount: Self::Coins,
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

#[derive(Clone)]
pub struct BankBalanceKeeper<S> {
    balance_store: JsonStore<SharedStore<S>, BalancesPath, Balances>,
}

impl<S: Store> BankKeeper for BankBalanceKeeper<S> {
    type Error = Error;
    type Address = AccountId;
    type Denom = Denom;
    type Coin = Coin;
    type Coins = Vec<Self::Coin>;

    fn send_coins(
        &mut self,
        from: Self::Address,
        to: Self::Address,
        amount: Self::Coins,
    ) -> Result<(), Self::Error> {
        let src_balance_path = BalancesPath(from);
        let mut src_balances = self
            .balance_store
            .get(Height::Pending, &src_balance_path)
            .map(|b| b.0)
            .unwrap_or_default();

        let dst_balance_path = BalancesPath(to);
        let mut dst_balances = self
            .balance_store
            .get(Height::Pending, &dst_balance_path)
            .map(|b| b.0)
            .unwrap_or_default();

        for Coin { denom, amount } in amount {
            let mut src_balance = src_balances
                .iter_mut()
                .find(|c| c.denom == denom)
                .ok_or_else(Error::insufficient_source_funds)?;

            if dst_balances.iter().any(|c| c.denom == denom) {
                dst_balances.push(Coin {
                    denom: denom.clone(),
                    amount: 0u64.into(),
                });
            }

            let mut dst_balance = dst_balances.iter_mut().find(|c| c.denom == denom).unwrap();

            if dst_balance.amount > U256::MAX - amount {
                return Err(Error::dest_fund_overflow());
            }

            src_balance.amount -= amount;
            dst_balance.amount += amount;
        }

        // Store the updated account balances
        self.balance_store
            .set(src_balance_path, Balances(src_balances))
            .map(|_| ())
            .map_err(|e| Error::store(format!("{:?}", e)))?;
        self.balance_store
            .set(dst_balance_path, Balances(dst_balances))
            .map(|_| ())
            .map_err(|e| Error::store(format!("{:?}", e)))?;

        Ok(())
    }

    fn mint_coins(
        &mut self,
        account: Self::Address,
        amount: Self::Coins,
    ) -> Result<(), Self::Error> {
        let balance_path = BalancesPath(account);
        let mut balances = self
            .balance_store
            .get(Height::Pending, &balance_path)
            .map(|b| b.0)
            .unwrap_or_default();

        for Coin { denom, amount } in amount {
            let mut balance = if let Some(i) = balances.iter_mut().position(|c| c.denom == denom) {
                &mut balances[i]
            } else {
                balances.push(Coin {
                    denom,
                    amount: 0u64.into(),
                });
                balances.last_mut().unwrap()
            };

            if balance.amount > U256::MAX - amount {
                return Err(Error::dest_fund_overflow());
            }

            balance.amount += amount;
        }

        // Store the updated account balances
        self.balance_store
            .set(balance_path, Balances(balances))
            .map(|_| ())
            .map_err(|e| Error::store(format!("{:?}", e)))?;

        Ok(())
    }

    fn burn_coins(
        &mut self,
        account: Self::Address,
        amount: Self::Coins,
    ) -> Result<(), Self::Error> {
        let balance_path = BalancesPath(account);
        let mut balances = self
            .balance_store
            .get(Height::Pending, &balance_path)
            .map(|b| b.0)
            .unwrap_or_default();

        for Coin { denom, amount } in amount {
            let mut balance = balances
                .iter_mut()
                .find(|c| c.denom == denom)
                .filter(|c| c.amount >= amount)
                .ok_or_else(Error::insufficient_source_funds)?;

            balance.amount -= amount;
        }

        // Store the updated account balances
        self.balance_store
            .set(balance_path, Balances(balances))
            .map(|_| ())
            .map_err(|e| Error::store(format!("{:?}", e)))?;

        Ok(())
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

    pub fn bank_keeper(&self) -> &BankBalanceKeeper<S> {
        &self.balance_keeper
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

    fn deliver(&mut self, message: Any) -> Result<Vec<Event>, ModuleError> {
        let message: MsgSend = Self::decode::<proto::cosmos::bank::v1beta1::MsgSend>(message)?
            .try_into()
            .map_err(|e| Error::msg_validation_failure(format!("{:?}", e)))?;

        let _ = self
            .account_reader
            .get_account(message.from_address.clone().into())
            .map_err(|_| Error::non_existent_account(message.from_address.clone()))?;

        // Note: we allow transfers to non-existent destination accounts
        let amounts: Vec<Coin> = message.amount.iter().map(|amt| amt.into()).collect();
        self.balance_keeper
            .send_coins(message.from_address, message.to_address, amounts)?;

        Ok(vec![])
    }

    fn init(&mut self, app_state: serde_json::Value) {
        debug!("Initializing bank module");

        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let accounts: HashMap<String, HashMap<Denom, U256>> =
            serde_json::from_value(app_state).unwrap();
        for (account, balances) in accounts {
            trace!("Adding account ({}) => {:?}", account, balances);

            let account_id = AccountId::from_str(&account).unwrap();
            self.balance_keeper
                .mint_coins(account_id, balances.into_iter().map(|b| b.into()).collect())
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
