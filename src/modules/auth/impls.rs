use std::{collections::HashMap, str::FromStr};

use crate::modules::auth::account::AuthAccount;
use crate::modules::bank::util::Denom;
use crate::{
    error::Error as AppError,
    helper::Height,
    modules::Module,
    store::{ProtobufStore, ProvableStore, SharedStore, Store, TypedStore},
};
use cosmrs::AccountId;
use ibc_proto::{
    cosmos::auth::v1beta1::{query_server::QueryServer, BaseAccount},
    google::protobuf::Any,
};
use serde_json::Value;
use tendermint_proto::abci::Event;
use tracing::{debug, trace};

use super::account::AccountsPath;
use super::{
    context::{Account, AccountKeeper, AccountReader},
    service::AuthService,
};

#[derive(Clone)]
pub struct Auth<S> {
    store: SharedStore<S>,
    account_reader: AuthAccountReader<S>,
    account_keeper: AuthAccountKeeper<S>,
}

impl<S: 'static + ProvableStore> Auth<S> {
    pub fn new(store: SharedStore<S>) -> Self {
        Self {
            store: store.clone(),
            account_reader: AuthAccountReader {
                account_store: TypedStore::new(store.clone()),
            },
            account_keeper: AuthAccountKeeper {
                account_store: TypedStore::new(store),
            },
        }
    }

    pub fn service(&self) -> QueryServer<AuthService<S>> {
        QueryServer::new(AuthService {
            account_reader: self.account_reader().clone(),
        })
    }

    pub fn account_reader(&self) -> &AuthAccountReader<S> {
        &self.account_reader
    }

    pub fn account_keeper(&self) -> &AuthAccountKeeper<S> {
        &self.account_keeper
    }
}

impl<S: Store> Module for Auth<S> {
    type Store = S;

    fn init(&mut self, app_state: Value) {
        debug!("Initializing auth module");
        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let accounts: HashMap<String, HashMap<Denom, String>> =
            serde_json::from_value(app_state).unwrap();
        for (account, _) in accounts {
            trace!("Adding account: {}", account);

            let account_id = AccountId::from_str(&account).unwrap();
            self.account_keeper
                .set_account(AuthAccount::new(account_id.clone()))
                .map_err(|_| "Failed to create account")
                .unwrap();
        }
    }

    fn deliver(&mut self, _message: Any, signer: &AccountId) -> Result<Vec<Event>, AppError> {
        let mut account = self
            .account_reader
            .get_account(signer.clone())
            .map_err(|_| AppError::Custom {
                reason: "unknown signer".to_string(),
            })?;
        account.sequence += 1;

        self.account_keeper
            .set_account(account)
            .map_err(|_| AppError::Custom {
                reason: "failed to increment signer sequence".to_string(),
            })?;

        // we're only intercepting the deliverTx here, so return unhandled.
        Err(AppError::NotHandled)
    }

    fn store_mut(&mut self) -> &mut SharedStore<S> {
        &mut self.store
    }

    fn store(&self) -> &SharedStore<S> {
        &self.store
    }
}

#[derive(Clone)]
pub struct AuthAccountReader<S> {
    account_store: ProtobufStore<SharedStore<S>, AccountsPath, AuthAccount, BaseAccount>,
}

impl<S: Store> AccountReader for AuthAccountReader<S> {
    type Error = ();
    type Address = AccountId;
    type Account = AuthAccount;

    fn get_account(&self, address: Self::Address) -> Result<Self::Account, Self::Error> {
        self.account_store
            .get(Height::Pending, &AccountsPath(address))
            .ok_or(())
    }
}

#[derive(Clone)]
pub struct AuthAccountKeeper<S> {
    account_store: ProtobufStore<SharedStore<S>, AccountsPath, AuthAccount, BaseAccount>,
}

impl<S: Store> AccountKeeper for AuthAccountKeeper<S> {
    type Error = ();
    type Account = AuthAccount;

    fn set_account(&mut self, account: Self::Account) -> Result<(), Self::Error> {
        self.account_store
            .set(AccountsPath(account.address().clone()), account)
            .map(|_| ())
            .map_err(|_| ())
    }

    fn remove_account(&mut self, _account: Self::Account) -> Result<(), Self::Error> {
        unimplemented!()
    }
}
