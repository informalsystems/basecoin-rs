use crate::app::modules::bank::Denom;
use crate::app::modules::Module;
use crate::app::store::{
    Height, Path, ProtobufStore, ProvableStore, SharedStore, Store, TypedStore,
};

use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::str::FromStr;

use cosmrs::AccountId;
use ibc_proto::cosmos::auth::v1beta1::BaseAccount;
use ibc_proto::cosmos::auth::v1beta1::{
    query_server::{Query, QueryServer},
    QueryAccountRequest, QueryAccountResponse, QueryAccountsRequest, QueryAccountsResponse,
    QueryParamsRequest, QueryParamsResponse,
};
use ibc_proto::google::protobuf::Any;
use prost::Message;
use serde_json::Value;
use tendermint_proto::Protobuf;
use tonic::{Request, Response, Status};
use tracing::{debug, trace};

/// Address of the account that the relayer uses to sign basecoin transactions.
/// This is hardcoded as we don't verify signatures currently.
const RELAYER_ACCOUNT: &str = "cosmos1snd5m4h0wt5ur55d47vpxla389r2xkf8dl6g9w";

#[derive(Clone)]
struct AccountsPath(AccountId);

impl From<AccountsPath> for Path {
    fn from(path: AccountsPath) -> Self {
        format!("accounts/{}", path.0).try_into().unwrap() // safety - cannot fail as AccountsPath is correct-by-construction
    }
}

pub trait Account {
    /// Account address type
    type Address;
    /// Account public key type
    type PubKey;

    /// Returns the account's address.
    fn address(&self) -> &Self::Address;

    /// Returns the account's public key.
    fn pub_key(&self) -> &Self::PubKey;

    /// Returns the account's sequence. (used for replay protection)
    fn sequence(&self) -> u64;
}

#[derive(Clone)]
pub struct AuthAccount {
    address: AccountId,
    number: u64,
    sequence: u64,
}

impl AuthAccount {
    pub fn new(address: AccountId) -> Self {
        Self {
            address,
            number: 0,
            sequence: 0,
        }
    }
}

impl Account for AuthAccount {
    type Address = AccountId;
    type PubKey = Vec<u8>;

    fn address(&self) -> &Self::Address {
        &self.address
    }

    fn pub_key(&self) -> &Self::PubKey {
        unimplemented!()
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }
}

impl Protobuf<BaseAccount> for AuthAccount {}

impl TryFrom<BaseAccount> for AuthAccount {
    type Error = String;

    fn try_from(account: BaseAccount) -> Result<Self, Self::Error> {
        Ok(AuthAccount {
            address: account
                .address
                .parse()
                .map_err(|_| "Failed to parse address".to_string())?,
            number: account.account_number,
            sequence: account.sequence,
        })
    }
}

impl From<AuthAccount> for BaseAccount {
    fn from(account: AuthAccount) -> Self {
        BaseAccount {
            address: account.address.to_string(),
            pub_key: None,
            account_number: account.number,
            sequence: account.sequence,
        }
    }
}

impl From<AuthAccount> for Any {
    fn from(account: AuthAccount) -> Self {
        let account = BaseAccount::from(account);
        Any {
            type_url: "/cosmos.auth.v1beta1.BaseAccount".to_string(),
            value: account.encode_to_vec(),
        }
    }
}

pub trait AccountReader {
    type Error;
    type Address;
    type Account: Account;

    fn get_account(&self, address: Self::Address) -> Result<Self::Account, Self::Error>;
}

pub trait AccountKeeper {
    type Error;
    type Account: Account;

    fn set_account(&mut self, account: Self::Account) -> Result<(), Self::Error>;

    fn remove_account(&mut self, account: Self::Account) -> Result<(), Self::Error>;
}

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
        let accounts: HashMap<String, HashMap<Denom, u64>> =
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

pub struct AuthService<S> {
    account_reader: AuthAccountReader<S>,
}

#[tonic::async_trait]
impl<S: ProvableStore + 'static> Query for AuthService<S> {
    async fn accounts(
        &self,
        _request: Request<QueryAccountsRequest>,
    ) -> Result<Response<QueryAccountsResponse>, Status> {
        unimplemented!()
    }

    async fn account(
        &self,
        _request: Request<QueryAccountRequest>,
    ) -> Result<Response<QueryAccountResponse>, Status> {
        debug!("Got auth account request");

        let account_id = RELAYER_ACCOUNT.parse().unwrap();
        let mut account = self.account_reader.get_account(account_id).unwrap();
        account.sequence += 1;

        Ok(Response::new(QueryAccountResponse {
            account: Some(account.into()),
        }))
    }

    async fn params(
        &self,
        _request: Request<QueryParamsRequest>,
    ) -> Result<Response<QueryParamsResponse>, Status> {
        unimplemented!()
    }
}
