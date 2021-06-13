//! Internal commands sent between the handle and the driver of the application.

use crate::app::state::{Balances, Store};
use crate::result::Result;
use cosmos_sdk::Coin;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub enum Command {
    /// Initialize the state of the basecoin app.
    Init {
        /// Initial balances of all the accounts.
        balances: Store,
        result_tx: Sender<Vec<u8>>,
    },
    /// Get the height of the last commit.
    GetInfo { result_tx: Sender<(i64, Vec<u8>)> },
    /// Get the balances associated with `account_id` (for all currency denominations).
    Get {
        account_id: String,
        result_tx: Sender<(i64, Option<Balances>)>,
    },
    /// Transfer an amount from one account to another.
    Transfer {
        src_account_id: String,
        dest_account_id: String,
        /// The amounts of each denomination of coin to transfer.
        amounts: Vec<Coin>,
        /// Channel that allows the driver to send (height, result)
        result_tx: Sender<(i64, Result<()>)>,
    },
    /// Commit the current state of the application, which involves recomputing
    /// the application's hash.
    Commit { result_tx: Sender<(i64, Vec<u8>)> },
}
