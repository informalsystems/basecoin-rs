#[cfg(any(feature = "v0_37", feature = "v0_38"))]
pub(crate) trait ResponseFromErrorExt {
    fn from_error(code: u32, log: impl ToString) -> Self;
}

#[cfg(any(feature = "v0_37", feature = "v0_38"))]
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

#[cfg(feature = "v0_37")]
const _: () = {
    use tendermint_proto::v0_37::abci::{ResponseCheckTx, ResponseDeliverTx, ResponseQuery};
    impl_response_error_for!(ResponseQuery, ResponseCheckTx, ResponseDeliverTx);
};

#[cfg(feature = "v0_38")]
const _: () = {
    use tendermint_proto::abci::{ResponseCheckTx, ResponseQuery};
    impl_response_error_for!(ResponseQuery, ResponseCheckTx);
};
