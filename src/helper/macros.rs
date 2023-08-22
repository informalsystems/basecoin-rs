use super::Path;
use crate::modules::{gov::path::ProposalPath, upgrade::path::UpgradePlanPath};

use ibc::core::ics24_host::path::{
    AckPath, ChannelEndPath, ClientConnectionPath, ClientConsensusStatePath, ClientStatePath,
    CommitmentPath, ConnectionPath, ReceiptPath, SeqAckPath, SeqRecvPath, SeqSendPath,
    UpgradeClientPath,
};

#[cfg(all(feature = "v0_37", not(feature = "v0_38")))]
use tendermint_proto::v0_37::abci::{ResponseCheckTx, ResponseDeliverTx, ResponseQuery};

#[cfg(any(feature = "v0_38", not(feature = "v0_37")))]
use tendermint_proto::abci::{ResponseCheckTx, ResponseQuery};

pub(crate) trait ResponseFromErrorExt {
    fn from_error(code: u32, log: impl ToString) -> Self;
}

macro_rules! impl_response_error_for {
    ($($resp:ty),+) => {
        $(impl ResponseFromErrorExt for $resp {
            fn from_error(code: u32, log: impl ToString) -> Self {
                let log = log.to_string();
                Self {
                    code,
                    log,
                    ..Self::default()
                }
            }
        })+
    };
}

#[cfg(all(feature = "v0_37", not(feature = "v0_38")))]
impl_response_error_for!(ResponseQuery, ResponseCheckTx, ResponseDeliverTx);

#[cfg(any(feature = "v0_38", not(feature = "v0_37")))]
impl_response_error_for!(ResponseQuery, ResponseCheckTx);

macro_rules! impl_into_path_for {
    ($($path:ty),+) => {
        $(impl From<$path> for Path {
            fn from(ibc_path: $path) -> Self {
                Self::try_from(ibc_path.to_string()).unwrap() // safety - `IbcPath`s are correct-by-construction
            }
        })+
    };
}

impl_into_path_for!(
    ClientStatePath,
    ClientConsensusStatePath,
    ConnectionPath,
    ClientConnectionPath,
    ChannelEndPath,
    SeqSendPath,
    SeqRecvPath,
    SeqAckPath,
    CommitmentPath,
    ReceiptPath,
    AckPath,
    UpgradeClientPath,
    UpgradePlanPath,
    ProposalPath
);
