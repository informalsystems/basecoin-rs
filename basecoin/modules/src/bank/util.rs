use basecoin_store::types::Path;
use cosmrs::{AccountId, Coin as MsgCoin};
use ibc_proto::cosmos::base::v1beta1::Coin as ProtoCoin;
use ibc_proto::Protobuf;
use primitive_types::U256;
use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone, Hash, Eq)]
#[serde(transparent)]
pub struct Denom(pub String);

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Coin {
    pub denom: Denom,
    pub amount: U256,
}

impl Coin {
    pub fn new_empty(denom: Denom) -> Self {
        Self {
            denom,
            amount: 0u64.into(),
        }
    }
}

impl Protobuf<ProtoCoin> for Coin {}

impl TryFrom<ProtoCoin> for Coin {
    type Error = Error;

    fn try_from(raw: ProtoCoin) -> Result<Self, Self::Error> {
        let amount = U256::from_str_radix(&raw.amount, 10).map_err(|e| Error::Custom {
            reason: e.to_string(),
        })?;

        Ok(Self {
            denom: Denom(raw.denom),
            amount,
        })
    }
}

impl From<Coin> for ProtoCoin {
    fn from(value: Coin) -> Self {
        Self {
            denom: value.denom.0,
            amount: value.amount.to_string(),
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
pub struct Balances(pub Vec<Coin>);

#[derive(Clone, Debug, derive_more::Display)]
pub(super) struct BalancesPath(pub AccountId);

impl From<BalancesPath> for Path {
    fn from(path: BalancesPath) -> Self {
        format!("balances/{}", path.0).try_into().unwrap() // safety - cannot fail as AccountsPath is correct-by-construction
    }
}
