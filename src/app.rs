//! The basecoin ABCI application.

mod response;
mod state;
mod tx;

use crate::app::response::ResponseFromErrorExt;
use crate::app::state::{BaseCoinState, Store};
use crate::app::tx::validate_tx;
use std::sync::{Arc, RwLock};
use tendermint_abci::Application;
use tendermint_proto::abci::{
    RequestCheckTx, RequestDeliverTx, RequestInfo, RequestInitChain, RequestQuery, ResponseCheckTx,
    ResponseCommit, ResponseDeliverTx, ResponseInfo, ResponseInitChain, ResponseQuery,
};
use tracing::{debug, info};

/// BaseCoin ABCI application.
///
/// Can be safely cloned and sent across threads, but not shared.
#[derive(Debug, Clone)]
pub struct BaseCoinApp {
    state: Arc<RwLock<BaseCoinState>>,
}

impl BaseCoinApp {
    /// Constructor.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(BaseCoinState::default())),
        }
    }
}

impl Application for BaseCoinApp {
    fn info(&self, request: RequestInfo) -> ResponseInfo {
        debug!(
            "Got info request. Tendermint version: {}; Block version: {}; P2P version: {}",
            request.version, request.block_version, request.p2p_version
        );

        let (last_block_height, last_block_app_hash) = {
            let state = self.state.read().unwrap();
            (state.height(), state.hash())
        };

        ResponseInfo {
            data: "basecoin-rs".to_string(),
            version: "0.1.0".to_string(),
            app_version: 1,
            last_block_height,
            last_block_app_hash,
        }
    }

    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        let balances: Store =
            serde_json::from_str(&String::from_utf8(request.app_state_bytes).unwrap()).unwrap();
        let app_hash = {
            let mut state = self.state.write().unwrap();
            state.set_balances(balances);
            state.hash()
        };

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: request.validators,
            app_hash,
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        let account_id = match String::from_utf8(request.data.clone()) {
            Ok(s) => s,
            Err(e) => panic!("Failed to interpret key as UTF-8: {}", e),
        };
        debug!("Attempting to get account ID: {}", account_id);
        let state = self.state.read().unwrap();
        match state.get_balances(&account_id) {
            Some(balances) => ResponseQuery {
                code: 0,
                log: "exists".to_string(),
                info: "".to_string(),
                index: 0,
                key: request.data,
                value: serde_json::to_string(&balances).unwrap().into_bytes(),
                proof_ops: None,
                height: state.height(),
                codespace: "".to_string(),
            },
            None => ResponseQuery::from_error(1, "does not exist"),
        }
    }

    fn check_tx(&self, request: RequestCheckTx) -> ResponseCheckTx {
        match validate_tx(request.tx) {
            Ok(_) => ResponseCheckTx::default(),
            Err((code, log)) => ResponseCheckTx::from_error(code, log),
        }
    }

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        let msg = match validate_tx(request.tx) {
            Ok(msg) => msg,
            Err((code, log)) => return ResponseDeliverTx::from_error(code, log),
        };
        debug!("Got MsgSend = {:?}", msg);
        let mut state = self.state.write().unwrap();
        match state.transfer(
            &msg.from_address.to_string(),
            &msg.to_address.to_string(),
            msg.amount,
        ) {
            Ok(_) => ResponseDeliverTx {
                log: "success".to_owned(),
                ..ResponseDeliverTx::default()
            },
            Err(e) => ResponseDeliverTx::from_error(10, e.to_string()),
        }
    }

    fn commit(&self) -> ResponseCommit {
        let (height, app_hash) = {
            let mut state = self.state.write().unwrap();
            state.commit()
        };
        info!("Committed height {}", height);
        ResponseCommit {
            data: app_hash,
            retain_height: 0,
        }
    }
}
