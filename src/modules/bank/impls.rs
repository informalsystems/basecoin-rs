use super::context::{BankKeeper, BankReader};
use super::error::Error;
use super::service::BankService;
use super::util::{Balances, BalancesPath, Coin, Denom};
use cosmrs::{bank::MsgSend, proto, AccountId};
use ibc_proto::{cosmos::bank::v1beta1::query_server::QueryServer, google::protobuf::Any};
use primitive_types::U256;
use prost::Message;
use std::{collections::HashMap, convert::TryInto, fmt::Debug, str::FromStr};
use tendermint_proto::abci::Event;
use tracing::{debug, trace};

use crate::modules::Module;
use crate::{
    error::Error as AppError,
    helper::{Height, Path, QueryResult},
    modules::auth::account::{AuthAccount, ACCOUNT_PREFIX},
    modules::auth::context::{AccountKeeper, AccountReader},
    store::{
        SharedStore,
        {codec::JsonCodec, Codec},
        {JsonStore, TypedStore}, {ProvableStore, Store},
    },
};

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
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
                .ok_or(Error::InsufficientSourceFunds)?;

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
                .ok_or(Error::InsufficientSourceFunds)?;

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
#[derive(Clone, Debug)]
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
    fn decode<T: Message + Default>(message: Any) -> Result<T, AppError> {
        if message.type_url != "/cosmos.bank.v1beta1.MsgSend" {
            return Err(AppError::NotHandled);
        }
        Message::decode(message.value.as_ref()).map_err(|_| Error::MsgDecodeFailure.into())
    }
}

impl<S: ProvableStore, AR: AccountReader + Send + Sync, AK: AccountKeeper + Send + Sync> Module
    for Bank<S, AR, AK>
where
    <AR as AccountReader>::Address: From<AccountId>,
    <AK as AccountKeeper>::Account: From<AuthAccount>,
{
    type Store = S;

    fn deliver(&mut self, message: Any, _signer: &AccountId) -> Result<Vec<Event>, AppError> {
        let message: MsgSend = Self::decode::<proto::cosmos::bank::v1beta1::MsgSend>(message)?
            .try_into()
            .map_err(|e| Error::MsgValidationFailure {
                reason: format!("{e:?}"),
            })?;
        self.account_reader
            .get_account(message.from_address.clone().into())
            .map_err(|_| Error::NonExistentAccount {
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
    ) -> Result<QueryResult, AppError> {
        let account_id = match String::from_utf8(data.to_vec()) {
            Ok(s) if s.starts_with(ACCOUNT_PREFIX) => s, // TODO(hu55a1n1): check if valid identifier
            _ => return Err(AppError::NotHandled),
        };

        let account_id = AccountId::from_str(&account_id).map_err(|_| AppError::NotHandled)?;

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

    fn check(&self, _message: Any) -> Result<(), AppError> {
        Ok(())
    }

    fn begin_block(&mut self, _header: &tendermint::block::Header) -> Vec<Event> {
        vec![]
    }
}
