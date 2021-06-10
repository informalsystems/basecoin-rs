//! Internal commands sent between the handle and the driver of the application.

use std::collections::HashMap;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub enum Command {
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
