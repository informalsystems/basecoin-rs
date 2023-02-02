use std::{collections::HashMap, convert::TryInto, fmt::Debug, num::ParseIntError, str::FromStr};

use cosmrs::{bank::MsgSend, proto, AccountId, Coin as MsgCoin};
use displaydoc::Display;
use ibc_proto::{
    cosmos::{
        bank::v1beta1::{
            query_server::{Query, QueryServer},
            QueryAllBalancesRequest, QueryAllBalancesResponse, QueryBalanceRequest,
            QueryBalanceResponse, QueryDenomMetadataRequest, QueryDenomMetadataResponse,
            QueryDenomOwnersRequest, QueryDenomOwnersResponse, QueryDenomsMetadataRequest,
            QueryDenomsMetadataResponse, QueryParamsRequest, QueryParamsResponse,
            QuerySpendableBalancesRequest, QuerySpendableBalancesResponse, QuerySupplyOfRequest,
            QuerySupplyOfResponse, QueryTotalSupplyRequest, QueryTotalSupplyResponse,
        },
        base::v1beta1::Coin as RawCoin,
    },
    google::protobuf::Any,
};
use primitive_types::U256;
use prost::Message;
use serde::{Deserialize, Serialize};
use tendermint_proto::abci::Event;
use tonic::{Request, Response, Status};
use tracing::{debug, trace};

use crate::app::{
    modules::{
        auth::{AccountKeeper, AccountReader, AuthAccount, ACCOUNT_PREFIX},
        Error as ModuleError, Module, QueryResult,
    },
    store::{
        Codec, Height, JsonCodec, JsonStore, Path, ProvableStore, SharedStore, Store, TypedStore,
    },
};

#[derive(Debug, Display)]
pub enum Error {
    /// failed to decode message
    MsgDecodeFailure,
    /// failed to validate message: `{reason}`
    MsgValidationFailure { reason: String },
    /// account `{account}` doesn't exist
    NonExistentAccount { account: AccountId },
    /// invalid amount specified
    InvalidAmount,
    /// insufficient funds in sender account
    InsufficientSourceFunds,
    /// receiver account funds overflow
    DestFundOverflow,
    /// Store error: `{reason}`
    Store { reason: String },
}

impl From<Error> for ModuleError {
    fn from(e: Error) -> Self {
        ModuleError::Bank(e)
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

impl Coin {
    fn new_empty(denom: Denom) -> Self {
        Self {
            denom,
            amount: 0u64.into(),
        }
    }
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
    type Error: Debug;
    type Address: FromStr;
    type Denom;
    type Coin;

    /// This function should enable sending ibc fungible tokens from one account to another
    fn send_coins(
        &mut self,
        from: Self::Address,
        to: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
    ) -> Result<(), Self::Error>;

    /// This function to enable minting ibc tokens to a user account
    fn mint_coins(
        &mut self,
        account: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
    ) -> Result<(), Self::Error>;

    /// This function should enable burning of minted tokens in a user account
    fn burn_coins(
        &mut self,
        account: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone)]
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

    fn send_coins(
        &mut self,
        from: Self::Address,
        to: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
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
            let src_balance = src_balances
                .iter_mut()
                .find(|c| c.denom == denom)
                .filter(|c| c.amount >= amount)
                .ok_or_else(|| Error::InsufficientSourceFunds)?;

            let dst_balance =
                if let Some(balance) = dst_balances.iter_mut().find(|c| c.denom == denom) {
                    balance
                } else {
                    dst_balances.push(Coin::new_empty(denom));
                    dst_balances.last_mut().unwrap()
                };

            if dst_balance.amount > U256::MAX - amount {
                return Err(Error::DestFundOverflow);
            }

            src_balance.amount -= amount;
            dst_balance.amount += amount;
        }

        // Store the updated account balances
        self.balance_store
            .set(src_balance_path, Balances(src_balances))
            .map(|_| ())
            .map_err(|e| Error::Store {
                reason: format!("{e:?}"),
            })?;
        self.balance_store
            .set(dst_balance_path, Balances(dst_balances))
            .map(|_| ())
            .map_err(|e| Error::Store {
                reason: format!("{e:?}"),
            })?;

        Ok(())
    }

    fn mint_coins(
        &mut self,
        account: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
    ) -> Result<(), Self::Error> {
        let balance_path = BalancesPath(account);
        let mut balances = self
            .balance_store
            .get(Height::Pending, &balance_path)
            .map(|b| b.0)
            .unwrap_or_default();

        for Coin { denom, amount } in amount {
            let balance = if let Some(i) = balances.iter_mut().position(|c| c.denom == denom) {
                &mut balances[i]
            } else {
                balances.push(Coin {
                    denom,
                    amount: 0u64.into(),
                });
                balances.last_mut().unwrap()
            };

            if balance.amount > U256::MAX - amount {
                return Err(Error::DestFundOverflow);
            }

            balance.amount += amount;
        }

        // Store the updated account balances
        self.balance_store
            .set(balance_path, Balances(balances))
            .map(|_| ())
            .map_err(|e| Error::Store {
                reason: format!("{e:?}"),
            })?;

        Ok(())
    }

    fn burn_coins(
        &mut self,
        account: Self::Address,
        amount: impl IntoIterator<Item = Self::Coin>,
    ) -> Result<(), Self::Error> {
        let balance_path = BalancesPath(account);
        let mut balances = self
            .balance_store
            .get(Height::Pending, &balance_path)
            .map(|b| b.0)
            .unwrap_or_default();

        for Coin { denom, amount } in amount {
            let balance = balances
                .iter_mut()
                .find(|c| c.denom == denom)
                .filter(|c| c.amount >= amount)
                .ok_or_else(|| Error::InsufficientSourceFunds)?;

            balance.amount -= amount;
        }

        // Store the updated account balances
        self.balance_store
            .set(balance_path, Balances(balances))
            .map(|_| ())
            .map_err(|e| Error::Store {
                reason: format!("{e:?}"),
            })?;

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

impl<S: 'static + ProvableStore + Default, AR: AccountReader, AK: AccountKeeper> Bank<S, AR, AK> {
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

    pub fn service(&self) -> QueryServer<BankService<S>> {
        QueryServer::new(BankService {
            bank_reader: self.balance_reader.clone(),
        })
    }

    pub fn bank_keeper(&self) -> &BankBalanceKeeper<S> {
        &self.balance_keeper
    }
}

impl<S: Store, AR: AccountReader, AK: AccountKeeper> Bank<S, AR, AK> {
    fn decode<T: Message + Default>(message: Any) -> Result<T, ModuleError> {
        if message.type_url != "/cosmos.bank.v1beta1.MsgSend" {
            return Err(ModuleError::NotHandled);
        }
        Message::decode(message.value.as_ref()).map_err(|_| Error::MsgDecodeFailure.into())
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
            .map_err(|e| Error::MsgValidationFailure {
                reason: format!("{e:?}"),
            })?;

        let _ = self
            .account_reader
            .get_account(message.from_address.clone().into())
            .map_err(|e| Error::NonExistentAccount {
                account: message.from_address.clone(),
            })?;

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
                .mint_coins(
                    account_id,
                    balances
                        .into_iter()
                        .map(|b| b.into())
                        .collect::<Vec<Coin>>(),
                )
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
            _ => return Err(ModuleError::NotHandled),
        };

        let account_id = AccountId::from_str(&account_id).map_err(|_| ModuleError::NotHandled)?;

        trace!("Attempting to get account ID: {}", account_id);

        let _ = self
            .account_reader
            .get_account(account_id.clone().into())
            .map_err(|_| Error::NonExistentAccount {
                account: account_id.clone(),
            })?;

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

pub struct BankService<S> {
    bank_reader: BankBalanceReader<S>,
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> Query for BankService<S> {
    async fn balance(
        &self,
        request: Request<QueryBalanceRequest>,
    ) -> Result<Response<QueryBalanceResponse>, Status> {
        debug!("Got bank balance request: {:?}", request);

        let account_id = request
            .get_ref()
            .address
            .parse()
            .map_err(|e| Status::invalid_argument(format!("{e}")))?;
        let denom = Denom(request.get_ref().denom.clone());
        let balances = self.bank_reader.get_all_balances(account_id);

        Ok(Response::new(QueryBalanceResponse {
            balance: balances
                .into_iter()
                .find(|c| c.denom == denom)
                .map(|coin| RawCoin {
                    denom: coin.denom.0,
                    amount: coin.amount.to_string(),
                }),
        }))
    }

    async fn all_balances(
        &self,
        _request: Request<QueryAllBalancesRequest>,
    ) -> Result<Response<QueryAllBalancesResponse>, Status> {
        unimplemented!()
    }

    async fn spendable_balances(
        &self,
        _request: Request<QuerySpendableBalancesRequest>,
    ) -> Result<Response<QuerySpendableBalancesResponse>, Status> {
        unimplemented!()
    }

    async fn total_supply(
        &self,
        _request: Request<QueryTotalSupplyRequest>,
    ) -> Result<Response<QueryTotalSupplyResponse>, Status> {
        unimplemented!()
    }

    async fn supply_of(
        &self,
        _request: Request<QuerySupplyOfRequest>,
    ) -> Result<Response<QuerySupplyOfResponse>, Status> {
        unimplemented!()
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }

    async fn denom_metadata(
        &self,
        _request: Request<QueryDenomMetadataRequest>,
    ) -> Result<Response<QueryDenomMetadataResponse>, Status> {
        unimplemented!()
    }

    async fn denoms_metadata(
        &self,
        _request: Request<QueryDenomsMetadataRequest>,
    ) -> Result<Response<QueryDenomsMetadataResponse>, Status> {
        unimplemented!()
    }

    async fn denom_owners(
        &self,
        _request: Request<QueryDenomOwnersRequest>,
    ) -> Result<Response<QueryDenomOwnersResponse>, Status> {
        unimplemented!()
    }
}
