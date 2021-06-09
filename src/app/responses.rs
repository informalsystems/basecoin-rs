//! Shorthand methods to produce ABCI responses.

use tendermint_proto::abci::{ResponseCheckTx, ResponseDeliverTx};

pub fn response_check_tx(code: u32, log: String) -> ResponseCheckTx {
    ResponseCheckTx {
        code,
        data: vec![],
        log,
        info: "".to_string(),
        gas_wanted: 0,
        gas_used: 0,
        events: vec![],
        codespace: "".to_string(),
    }
}

pub fn response_deliver_tx(code: u32, log: String) -> ResponseDeliverTx {
    ResponseDeliverTx {
        code,
        data: vec![],
        log,
        info: "".to_string(),
        gas_wanted: 0,
        gas_used: 0,
        events: vec![],
        codespace: "".to_string(),
    }
}
