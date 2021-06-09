use tendermint_proto::abci::{ResponseCheckTx, ResponseDeliverTx, ResponseQuery};

pub(crate) trait ResponseFromErrorExt {
    fn from_error(code: u32, log: impl ToString) -> Self;
}

macro_rules! impl_response_error_for {
    ($resp: ident) => {
        impl ResponseFromErrorExt for $resp {
            fn from_error(code: u32, log: impl ToString) -> Self {
                let log = log.to_string();
                Self {
                    code,
                    log,
                    ..Self::default()
                }
            }
        }
    };
}

impl_response_error_for!(ResponseQuery);
impl_response_error_for!(ResponseCheckTx);
impl_response_error_for!(ResponseDeliverTx);
