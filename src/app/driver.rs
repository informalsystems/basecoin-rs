//! The core driver of the basecoin application.

use crate::app::Command;
use crate::encoding::encode_varint;
use crate::sync::channel_send;
use bytes::BytesMut;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use tendermint_abci::{Error, Result};
use tracing::debug;

const APP_HASH_LENGTH: usize = 16;

#[derive(Debug)]
pub struct BaseCoinDriver {
    store: HashMap<String, u64>,
    height: i64,
    app_hash: Vec<u8>,
    cmd_rx: Receiver<Command>,
}

impl BaseCoinDriver {
    pub(crate) fn new(cmd_rx: Receiver<Command>) -> Self {
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
