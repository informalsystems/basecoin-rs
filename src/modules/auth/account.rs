use std::convert::{TryFrom, TryInto};

use crate::helper::Path;
use cosmrs::AccountId;
use ibc_proto::{cosmos::auth::v1beta1::BaseAccount, google::protobuf::Any};
use prost::Message;

use super::context::Account;

/// Address of the account that the relayer uses to sign basecoin transactions.
/// This is hardcoded as we don't verify signatures currently.
pub const RELAYER_ACCOUNT: &str = "cosmos12xpmzmfpf7tn57xg93rne2hc2q26lcfql5efws";
pub const ACCOUNT_PREFIX: &str = "cosmos";

#[derive(Clone)]
pub struct AccountsPath(pub AccountId);

impl From<AccountsPath> for Path {
    fn from(path: AccountsPath) -> Self {
        format!("accounts/{}", path.0).try_into().unwrap() // safety - cannot fail as AccountsPath is correct-by-construction
    }
}

#[derive(Clone)]
pub struct AuthAccount {
    address: AccountId,
    number: u64,
    pub sequence: u64,
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

impl ibc_proto::protobuf::Protobuf<BaseAccount> for AuthAccount {}

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
