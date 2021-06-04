//! Rudimentary basecoin application.

use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use tendermint_abci::{Application, Error, Result};
use tendermint_proto::abci::{
    RequestCheckTx, RequestDeliverTx, RequestInfo, RequestInitChain, RequestQuery, ResponseCheckTx,
    ResponseCommit, ResponseDeliverTx, ResponseInfo, ResponseInitChain, ResponseQuery,
};
use tracing::{debug, info};

const APP_HASH_LENGTH: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub sender: String,
    pub receiver: String,
    pub amount: u64,
}

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
            consensus_params: None,
            validators: vec![],
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
            retain_height: height - 1,
        }
    }
}

#[derive(Debug)]
pub struct BaseCoinDriver {
    store: HashMap<String, u64>,
    height: i64,
    app_hash: Vec<u8>,
    cmd_rx: Receiver<Command>,
}

impl BaseCoinDriver {
    fn new(cmd_rx: Receiver<Command>) -> Self {
        Self {
            store: HashMap::new(),
            height: 0,
            app_hash: vec![0_u8; APP_HASH_LENGTH],
            cmd_rx,
        }
    }

    /// Run the driver in the current thread (blocking).
    pub fn run(mut self) -> Result<()> {
        loop {
            let cmd = self
                .cmd_rx
                .recv()
                .map_err(|e| Error::ChannelRecv(e.to_string()))?;
            match cmd {
                Command::Init {
                    balances,
                    result_tx,
                } => {
                    self.store = balances;
                    debug!("Account balances initialized: {:?}", self.store);
                    channel_send(&result_tx, self.app_hash.clone())?;
                }
                Command::GetInfo { result_tx } => {
                    channel_send(&result_tx, (self.height, self.app_hash.clone()))?
                }
                Command::Get {
                    account_id: key,
                    result_tx,
                } => {
                    debug!("Getting value for \"{}\"", key);
                    channel_send(
                        &result_tx,
                        (self.height, self.store.get(&key).map(Clone::clone)),
                    )?;
                }
                Command::Commit { result_tx } => self.commit(result_tx)?,
                Command::Transfer {
                    src_account_id,
                    dest_account_id,
                    amount,
                    result_tx,
                } => {
                    debug!(
                        "Transfer request from {} to {} of amount {}",
                        src_account_id, dest_account_id, amount
                    );
                    let mut src_balance = match self.store.get(&src_account_id) {
                        Some(b) => *b,
                        None => {
                            channel_send(&result_tx, (self.height, false))?;
                            continue;
                        }
                    };
                    if amount > src_balance {
                        debug!(
                            "Source account does not have enough funds ({})",
                            src_balance
                        );
                        channel_send(&result_tx, (self.height, false))?;
                        continue;
                    }
                    let mut dest_balance = match self.store.get(&dest_account_id) {
                        Some(b) => *b,
                        None => {
                            self.store.insert(dest_account_id.clone(), 0);
                            0
                        }
                    };
                    src_balance -= amount;
                    dest_balance += amount;
                    self.store.insert(src_account_id.clone(), src_balance);
                    self.store.insert(dest_account_id.clone(), dest_balance);
                    debug!(
                        "New account balances: {} = {}, {} = {}",
                        src_account_id, src_balance, dest_account_id, dest_balance
                    );
                    channel_send(&result_tx, (self.height, true))?;
                }
            }
        }
    }

    fn commit(&mut self, result_tx: Sender<(i64, Vec<u8>)>) -> Result<()> {
        // As in the Go-based key/value store, simply encode the number of
        // items as the "app hash"
        let mut app_hash = BytesMut::with_capacity(APP_HASH_LENGTH);
        encode_varint(self.store.len() as u64, &mut app_hash);
        self.app_hash = app_hash.to_vec();
        self.height += 1;
        channel_send(&result_tx, (self.height, self.app_hash.clone()))
    }
}

#[derive(Debug, Clone)]
enum Command {
    /// Initialize the state of the basecoin app.
    Init {
        /// Initial balances of all the accounts.
        balances: HashMap<String, u64>,
        result_tx: Sender<Vec<u8>>,
    },
    /// Get the height of the last commit.
    GetInfo { result_tx: Sender<(i64, Vec<u8>)> },
    /// Get the balance associated with `account_id`.
    Get {
        account_id: String,
        result_tx: Sender<(i64, Option<u64>)>,
    },
    /// Transfer an amount from one account to another.
    Transfer {
        src_account_id: String,
        dest_account_id: String,
        amount: u64,
        /// Channel that allows the driver to send (height, success)
        result_tx: Sender<(i64, bool)>,
    },
    /// Commit the current state of the application, which involves recomputing
    /// the application's hash.
    Commit { result_tx: Sender<(i64, Vec<u8>)> },
}

fn channel_send<T>(tx: &Sender<T>, value: T) -> Result<()> {
    tx.send(value)
        .map_err(|e| Error::ChannelSend(e.to_string()).into())
}

fn channel_recv<T>(rx: &Receiver<T>) -> Result<T> {
    rx.recv()
        .map_err(|e| Error::ChannelRecv(e.to_string()).into())
}

fn encode_varint<B: BufMut>(val: u64, mut buf: &mut B) {
    prost::encoding::encode_varint(val << 1, &mut buf);
}
