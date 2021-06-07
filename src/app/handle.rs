//! The primary interface to the actual application.

use crate::app::{BaseCoinDriver, Command};
use crate::sync::{channel_recv, channel_send};
use crate::tx::Transaction;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use tendermint_abci::{Application, Result};
use tendermint_proto::abci::{
    RequestCheckTx, RequestDeliverTx, RequestInfo, RequestInitChain, RequestQuery, ResponseCheckTx,
    ResponseCommit, ResponseDeliverTx, ResponseInfo, ResponseInitChain, ResponseQuery,
};
use tracing::{debug, info};

/// The primary interface for an instance of the basecoin ABCI application.
///
/// This interface cannot be shared across threads, but can easily be cloned
/// across threads, since it is effectively just a handle to the actual
/// application machine (i.e. the `BaseCoinDriver`).
#[derive(Debug, Clone)]
pub struct BaseCoinApp {
    cmd_tx: Sender<Command>,
}

impl BaseCoinApp {
    /// Constructor.
    pub fn new() -> (Self, BaseCoinDriver) {
        let (cmd_tx, cmd_rx) = channel();
        (Self { cmd_tx }, BaseCoinDriver::new(cmd_rx))
    }

    /// Attempt to retrieve the value associated with the given key.
    pub fn get<K: AsRef<str>>(&self, key: K) -> Result<(i64, Option<u64>)> {
        let (result_tx, result_rx) = channel();
        channel_send(
            &self.cmd_tx,
            Command::Get {
                account_id: key.as_ref().to_string(),
                result_tx,
            },
        )?;
        channel_recv(&result_rx)
    }

    pub fn transfer(&self, sender: &str, receiver: &str, amount: u64) -> Result<(i64, bool)> {
        let (result_tx, result_rx) = channel();
        channel_send(
            &self.cmd_tx,
            Command::Transfer {
                src_account_id: sender.to_owned(),
                dest_account_id: receiver.to_owned(),
                amount,
                result_tx,
            },
        )?;
        channel_recv(&result_rx)
    }
}

impl Application for BaseCoinApp {
    fn info(&self, request: RequestInfo) -> ResponseInfo {
        debug!(
            "Got info request. Tendermint version: {}; Block version: {}; P2P version: {}",
            request.version, request.block_version, request.p2p_version
        );

        let (result_tx, result_rx) = channel();
        channel_send(&self.cmd_tx, Command::GetInfo { result_tx }).unwrap();
        let (last_block_height, last_block_app_hash) = channel_recv(&result_rx).unwrap();

        ResponseInfo {
            data: "basecoin-rs".to_string(),
            version: "0.1.0".to_string(),
            app_version: 1,
            last_block_height,
            last_block_app_hash,
        }
    }

    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        let balances: HashMap<String, u64> =
            serde_json::from_str(&String::from_utf8(request.app_state_bytes).unwrap()).unwrap();
        let (result_tx, result_rx) = channel();
        channel_send(
            &self.cmd_tx,
            Command::Init {
                balances,
                result_tx,
            },
        )
        .unwrap();
        let last_block_app_hash = channel_recv(&result_rx).unwrap();

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: request.validators,
            app_hash: last_block_app_hash,
        }
    }

    fn query(&self, request: RequestQuery) -> ResponseQuery {
        let account_id = match String::from_utf8(request.data.clone()) {
            Ok(s) => s,
            Err(e) => panic!("Failed to intepret key as UTF-8: {}", e),
        };
        debug!("Attempting to get account ID: {}", account_id);
        match self.get(account_id.clone()) {
            Ok((height, value_opt)) => match value_opt {
                Some(value) => ResponseQuery {
                    code: 0,
                    log: "exists".to_string(),
                    info: "".to_string(),
                    index: 0,
                    key: request.data,
                    value: format!("{}", value).into_bytes(),
                    proof_ops: None,
                    height,
                    codespace: "".to_string(),
                },
                None => ResponseQuery {
                    code: 0,
                    log: "does not exist".to_string(),
                    info: "".to_string(),
                    index: 0,
                    key: request.data,
                    value: vec![],
                    proof_ops: None,
                    height,
                    codespace: "".to_string(),
                },
            },
            Err(e) => panic!("Failed to get key \"{}\": {:?}", account_id, e),
        }
    }

    fn check_tx(&self, _request: RequestCheckTx) -> ResponseCheckTx {
        ResponseCheckTx {
            code: 0,
            data: vec![],
            log: "".to_string(),
            info: "".to_string(),
            gas_wanted: 1,
            gas_used: 0,
            events: vec![],
            codespace: "".to_string(),
        }
    }

    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        let tx: Transaction = match serde_json::from_str(&String::from_utf8(request.tx).unwrap()) {
            Ok(tx) => tx,
            Err(e) => {
                return ResponseDeliverTx {
                    code: 1,
                    data: vec![],
                    log: e.to_string(),
                    info: "".to_string(),
                    gas_wanted: 0,
                    gas_used: 0,
                    events: vec![],
                    codespace: "".to_string(),
                }
            }
        };
        match self.transfer(&tx.sender, &tx.receiver, tx.amount) {
            Ok((_, success)) => {
                if success {
                    ResponseDeliverTx {
                        code: 0,
                        data: vec![],
                        log: "".to_string(),
                        info: "".to_string(),
                        gas_wanted: 0,
                        gas_used: 0,
                        events: vec![],
                        codespace: "".to_string(),
                    }
                } else {
                    ResponseDeliverTx {
                        code: 1,
                        data: vec![],
                        log: "source account does not exist or insufficient balance".to_owned(),
                        info: "".to_string(),
                        gas_wanted: 0,
                        gas_used: 0,
                        events: vec![],
                        codespace: "".to_string(),
                    }
                }
            }
            Err(e) => ResponseDeliverTx {
                code: 1,
                data: vec![],
                log: e.to_string(),
                info: "".to_string(),
                gas_wanted: 0,
                gas_used: 0,
                events: vec![],
                codespace: "".to_string(),
            },
        }
    }

    fn commit(&self) -> ResponseCommit {
        let (result_tx, result_rx) = channel();
        channel_send(&self.cmd_tx, Command::Commit { result_tx }).unwrap();
        let (height, app_hash) = channel_recv(&result_rx).unwrap();
        info!("Committed height {}", height);
        ResponseCommit {
            data: app_hash,
            retain_height: 0,
        }
    }
}
