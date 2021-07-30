//! The basecoin ABCI application.

pub mod modules;
mod response;
mod state;
pub mod store;
mod tx;

use crate::app::response::ResponseFromErrorExt;
use crate::app::state::{BaseCoinState, Store};
use crate::app::tx::validate_tx;
use ::ibc::events::IbcEvent;
use ::ibc::ics26_routing::error::Kind;
use ::ibc::ics26_routing::handler::deliver;
use cosmos_sdk::Tx;
use std::sync::{Arc, RwLock};
use tendermint_abci::Application;
use tendermint_proto::abci::{
    Event, EventAttribute, RequestDeliverTx, RequestInfo, RequestInitChain, RequestQuery,
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

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        // Decode the txs
        let tx = match Tx::from_bytes(&request.tx) {
            Ok(tx) => tx,
            Err(e) => {
                debug!("Failed to decode incoming tx bytes: {:?}", request.tx);
                return ResponseDeliverTx::from_error(1, e.to_string());
            }
        };

        let mut state = self.state.write().unwrap();
        match deliver(
            &mut state.store.context,
            tx.body
                .messages
                .clone()
                .into_iter()
                .map(|msg| msg.into())
                .collect(),
        ) {
            Ok(events) => {
                let events = events
                    .iter()
                    .filter_map(|e| match e {
                        IbcEvent::CreateClient(c) => Some(Event {
                            r#type: "create_client".to_string(),
                            attributes: vec![EventAttribute {
                                key: "client_id".as_bytes().to_vec(),
                                value: c.client_id().to_string().as_bytes().to_vec(),
                                index: false,
                            }],
                        }),
                        _ => None,
                    })
                    .collect();

                ResponseDeliverTx {
                    log: "success".to_string(),
                    events,
                    ..ResponseDeliverTx::default()
                }
            }
            Err(e) => match e.kind() {
                Kind::UnknownMessageTypeUrl(_) => {
                    let msg = match validate_tx(tx) {
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
                _ => ResponseDeliverTx::from_error(2, e.to_string()),
            },
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
