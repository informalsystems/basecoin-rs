//! Synchronization primitives.

use std::sync::mpsc::{Receiver, Sender};
use tendermint_abci::{Error, Result};

/// Send an arbitrary value to the given channel, automatically converting any
/// errors to our ABCI `Error` type.
pub fn channel_send<T>(tx: &Sender<T>, value: T) -> Result<()> {
    tx.send(value)
        .map_err(|e| Error::ChannelSend(e.to_string()).into())
}

/// Receive an arbitrary value from the given channel, automatically converting
/// any errors to our ABCI `Error` type.
pub fn channel_recv<T>(rx: &Receiver<T>) -> Result<T> {
    rx.recv()
        .map_err(|e| Error::ChannelRecv(e.to_string()).into())
}
