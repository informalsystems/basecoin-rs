//! The primary interface to the actual application.

use crate::app::{BaseCoinDriver, Command};
use crate::sync::{channel_recv, channel_send};
use cosmos_sdk::bank::MsgSend;
use cosmos_sdk::tx::MsgType;
use cosmos_sdk::{AccountId, Coin, Tx};
use std::collections::HashMap;
use std::str::FromStr;
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

    pub fn transfer(
        &self,
        sender: &AccountId,
        receiver: &AccountId,
        amount: Vec<Coin>,
    ) -> Result<(i64, bool)> {
        let (result_tx, result_rx) = channel();
        channel_send(
            &self.cmd_tx,
            Command::Transfer {
                src_account_id: sender.as_ref().to_owned(),
                dest_account_id: receiver.as_ref().to_owned(),
                amount: u64::from_str(amount[0].amount.to_string().as_str())?,
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

    fn check_tx(&self, request: RequestCheckTx) -> ResponseCheckTx {
        let tx = match Tx::from_bytes(request.tx.as_ref()) {
            Ok(tx) => tx,
            Err(e) => {
                debug!("Failed to decode incoming tx bytes: {:?}", request.tx);
                return ResponseCheckTx {
                    code: 1,
                    data: vec![],
                    log: e.to_string(),
                    info: "".to_string(),
                    gas_wanted: 0,
                    gas_used: 0,
                    events: vec![],
                    codespace: "".to_string(),
                };
            }
        };
        if tx.body.messages.is_empty() {
            debug!("Got empty tx body");
            return ResponseCheckTx {
                code: 2,
                data: vec![],
                log: "no messages in incoming transaction".to_string(),
                info: "".to_string(),
                gas_wanted: 0,
                gas_used: 0,
                events: vec![],
                codespace: "".to_string(),
            };
        }
        let msg = match MsgSend::from_msg(&tx.body.messages[0]) {
            Ok(m) => m,
            Err(e) => {
                debug!(
                    "Failed to decode a bank send tx from {:?}\n\n{:?}",
                    tx.body.messages[0], e
                );
                return ResponseCheckTx {
                    code: 3,
                    data: vec![],
                    log: e.to_string(),
                    info: "".to_string(),
                    gas_wanted: 0,
                    gas_used: 0,
                    events: vec![],
                    codespace: "".to_string(),
                };
            }
        };
        if let Err(e) = u64::from_str(msg.amount[0].amount.to_string().as_str()) {
            return ResponseCheckTx {
                code: 4,
                data: vec![],
                log: format!("failed to decode amount: {}", e.to_string()),
                info: "".to_string(),
                gas_wanted: 0,
                gas_used: 0,
                events: vec![],
                codespace: "".to_string(),
            };
        }
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
        let tx = match Tx::from_bytes(request.tx.as_ref()) {
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
        if tx.body.messages.is_empty() {
            return ResponseDeliverTx {
                code: 1,
                data: vec![],
                log: "no messages in incoming transaction".to_string(),
                info: "".to_string(),
                gas_wanted: 0,
                gas_used: 0,
                events: vec![],
                codespace: "".to_string(),
            };
        }
        let msg = match MsgSend::from_msg(&tx.body.messages[0]) {
            Ok(msg) => msg,
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
        debug!("Got MsgSend = {:?}", msg);
        match self.transfer(&msg.from_address, &msg.to_address, msg.amount) {
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
