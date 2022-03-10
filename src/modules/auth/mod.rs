mod account;
mod grpc;
mod keepers;
mod readers;
mod store;

pub use account::{Account, AccountId, AuthAccount};
use grpc::AuthQuery;
pub use keepers::{AccountKeeper, AuthAccountKeeper};
pub use readers::{AccountReader, AuthAccountReader};
use store::{AccountStore, AccountsPath};

use crate::modules::bank::Denom;
use crate::modules::Module;
use crate::prostgen::cosmos::auth::v1beta1::query_server::QueryServer;
use crate::prostgen::cosmos::auth::v1beta1::BaseAccount;
use crate::store::{ProvableStore, SharedStore, Store, TypedStore};

use std::collections::HashMap;
use std::str::FromStr;

use serde_json::Value;
use tracing::{debug, trace};

const RELAYER_ACCOUNT: &str = "cosmos1snd5m4h0wt5ur55d47vpxla389r2xkf8dl6g9w";

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

    pub fn query(&self) -> QueryServer<AuthQuery<S>> {
        QueryServer::new(AuthQuery {
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

impl<S: Store> Module<S> for Auth<S> {
    fn init(&mut self, app_state: Value) {
        debug!("Initializing auth module");
        // safety - we panic on errors to prevent chain creation with invalid genesis config
        let accounts: HashMap<String, HashMap<Denom, u64>> =
            serde_json::from_value(app_state).unwrap();
        for (account, balances) in accounts {
            trace!("Adding account ({}) => {:?}", account, balances);

            let account_id = AccountId::from_str(&account).unwrap();
            self.account_keeper
                .set_account(AuthAccount::new(account_id.clone()))
                .map_err(|_| "Failed to create account")
                .unwrap();
        }
    }

    fn store_mut(&mut self) -> &mut SharedStore<S> {
        &mut self.store
    }

    fn store(&self) -> &SharedStore<S> {
        &self.store
    }
}
