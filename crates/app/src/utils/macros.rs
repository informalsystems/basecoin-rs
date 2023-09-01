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
