//! The basecoin ABCI application.

pub mod modules;
mod response;
pub mod store;

use crate::app::modules::bank::Bank;
use crate::app::modules::ibc::Ibc;
use crate::app::modules::{prefix, Error, Module};
use crate::app::response::ResponseFromErrorExt;
use crate::app::store::{Height, Path, ProvableStore, SharedSubStore};
use cosmos_sdk::Tx;
use std::convert::{Into, TryInto};
use std::sync::{Arc, RwLock};
use tendermint_abci::Application;
use tendermint_proto::abci::{
    RequestDeliverTx, RequestInfo, RequestInitChain, RequestQuery, ResponseCommit,
    ResponseDeliverTx, ResponseInfo, ResponseInitChain, ResponseQuery,
};
use tracing::{debug, info};

/// BaseCoin ABCI application.
///
/// Can be safely cloned and sent across threads, but not shared.
#[derive(Clone)]
pub struct BaseCoinApp<S> {
    state: Arc<RwLock<S>>,
    modules: Arc<RwLock<Vec<Box<dyn Module + Send + Sync>>>>,
}

impl<S: Default + ProvableStore + 'static> BaseCoinApp<S> {
    /// Constructor.
    pub fn new() -> Self {
        let state = Arc::new(RwLock::new(Default::default()));
        let modules: Vec<Box<dyn Module + Send + Sync>> = vec![
            Box::new(Bank {
                store: SharedSubStore {
                    store: state.clone(),
                    path: prefix::Bank,
                },
            }),
            Box::new(Ibc {
                store: SharedSubStore {
                    store: state.clone(),
                    path: prefix::Ibc,
                },
                client_counter: 0,
            }),
        ];
        Self {
            state,
            modules: Arc::new(RwLock::new(modules)),
        }
    }
}

impl<S: ProvableStore + 'static> Application for BaseCoinApp<S> {
    fn info(&self, request: RequestInfo) -> ResponseInfo {
        let (last_block_height, last_block_app_hash) = {
            let state = self.state.read().unwrap();
            (state.current_height() as i64, state.root_hash())
        };
        debug!(
            "Got info request. Tendermint version: {}; Block version: {}; P2P version: {}, {:?}, {:?}",
            request.version, request.block_version, request.p2p_version, last_block_height, last_block_app_hash
        );
        ResponseInfo {
            data: "basecoin-rs".to_string(),
            version: "0.1.0".to_string(),
            app_version: 1,
            last_block_height,
            last_block_app_hash: vec![],
        }
    }

    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        debug!("Got init chain request.");

        let mut modules = self.modules.write().unwrap();
        for m in modules.iter_mut() {
            m.init(
                serde_json::from_str(&String::from_utf8(request.app_state_bytes.clone()).unwrap())
                    .unwrap(),
            )
        }

        // commit genesis state
        let mut state = self.state.write().unwrap();
        state.commit();

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: request.validators,
            // app_hash: state.root_hash().unwrap_or(Hash::from_bytes(Algorithm::Sha256, &[0u8;16]).unwrap()).as_bytes().to_vec()
            app_hash: state.root_hash(),
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        let state = self.state.read().unwrap();
        let modules = self.modules.read().unwrap();

        let path: Path = match request.path.try_into() {
            Err(e) => return ResponseQuery::from_error(1, format!("Invalid path: {:?}", e)),
            Ok(path) => path,
        };

        for m in modules.iter() {
            match m.query(&request.data, &path, Height::from(request.height as u64)) {
                Ok(result) => {
                    return ResponseQuery {
                        code: 0,
                        log: "exists".to_string(),
                        info: "".to_string(),
                        index: 0,
                        key: request.data,
                        value: result,
                        proof_ops: None,
                        height: state.current_height() as i64,
                        codespace: "".to_string(),
                    };
                }
                Err(e) if e.detail() == Error::unhandled().detail() => continue,
                Err(e) => return ResponseQuery::from_error(1, format!("query error: {:?}", e)),
            }
        }
        ResponseQuery::from_error(1, "query msg unhandled")
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

        let mut modules = self.modules.write().unwrap();
        for message in tx.body.messages {
            for m in modules.iter_mut() {
                match m.deliver(message.clone().into()) {
                    Ok(events) => {
                        return ResponseDeliverTx {
                            log: "success".to_owned(),
                            events,
                            ..ResponseDeliverTx::default()
                        };
                    }
                    Err(e) if e.detail() == Error::unhandled().detail() => continue,
                    Err(e) => return ResponseDeliverTx::from_error(2, format!("{:?}", e)),
                };
            }
        }
        ResponseDeliverTx::from_error(2, "Tx msg unhandled")
    }

    fn commit(&self) -> ResponseCommit {
        let mut state = self.state.write().unwrap();
        let data = state.commit();
        info!("Committed height {}", state.current_height());
        ResponseCommit {
            data,
            retain_height: 0,
        }
    }
}
