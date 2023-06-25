use super::Path;

use ibc::core::ics24_host::path::{
    AckPath, ChannelEndPath, ClientConnectionPath, ClientConsensusStatePath, ClientStatePath,
    CommitmentPath, ConnectionPath, ReceiptPath, SeqAckPath, SeqRecvPath, SeqSendPath,
    UpgradeClientPath,
};
use tendermint_proto::abci::{ResponseCheckTx, ResponseDeliverTx, ResponseQuery};

pub trait ResponseFromErrorExt {
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

impl_response_error_for!(ResponseQuery, ResponseCheckTx, ResponseDeliverTx);

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
    UpgradeClientPath
);
