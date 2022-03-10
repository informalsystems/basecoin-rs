use crate::prostgen::cosmos::auth::v1beta1::BaseAccount;

use std::convert::TryFrom;

use prost::Message;
use prost_types::Any;
use tendermint_proto::Protobuf;

pub type AccountId = cosmrs::AccountId;

pub trait Account {
    type Address;
    type PubKey;

    fn address(&self) -> &Self::Address;
    fn pub_key(&self) -> &Self::PubKey;
    fn number(&self) -> u64;
    fn sequence(&self) -> u64;
}

#[derive(Clone)]
pub struct AuthAccount {
    address: AccountId,
    number: u64,
    pub(super) sequence: u64,
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

    fn number(&self) -> u64 {
        self.number
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
