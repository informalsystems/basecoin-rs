//! The core driver of the basecoin application.

use crate::app::state::BaseCoinState;
use crate::app::Command;
use crate::sync::channel_send;
use std::sync::mpsc::Receiver;
use tendermint_abci::{Error, Result};
use tracing::debug;

/// The core state machine of the basecoin application.
///
/// It is exclusively accessible via its handle (i.e. the `BaseCoinApp`). All
/// incoming requests are effectively serialized into an unbounded channel and
/// are processed serially.
#[derive(Debug)]
pub struct BaseCoinDriver {
    state: BaseCoinState,
    cmd_rx: Receiver<Command>,
}

impl BaseCoinDriver {
    pub(crate) fn new(cmd_rx: Receiver<Command>) -> Self {
        Self {
            state: Default::default(),
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
                    self.state = BaseCoinState::new(balances);
                    debug!("Account balances initialized: {:?}", self.state);
                    channel_send(&result_tx, self.state.hash())?;
                }
                Command::GetInfo { result_tx } => {
                    channel_send(&result_tx, (self.state.height(), self.state.hash()))?
                }
                Command::Get {
                    account_id,
                    result_tx,
                } => {
                    debug!("Getting account balances for \"{}\"", account_id);
                    channel_send(
                        &result_tx,
                        (self.state.height(), self.state.get_balances(&account_id)),
                    )?;
                }
                Command::Commit { result_tx } => channel_send(&result_tx, self.state.commit())?,
                Command::Transfer {
                    src_account_id,
                    dest_account_id,
                    amounts,
                    result_tx,
                } => {
                    debug!(
                        "Transfer request from {} to {} of amounts: {:?}",
                        src_account_id, dest_account_id, amounts,
                    );
                    let result = self
                        .state
                        .transfer(&src_account_id, &dest_account_id, amounts);
                    channel_send(&result_tx, (self.state.height(), result))?;
                }
            }
        }
    }
}
