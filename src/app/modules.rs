pub mod bank;
pub mod ibc;

use crate::app::modules::bank::Bank;
use crate::app::modules::ibc::Ibc;
use crate::app::store::{Height, Path, Store};
use prost_types::Any;
use std::hash::Hash;
use tendermint_proto::abci::Event;

pub trait Module {
    /// Tries to decode a protobuf message to a module supported Message`
    /// This is used to determine if a message is handleable by this module or not
    /// Do NOT use for validation!
    // fn decode<T: Message + Default>(&self, message: Any) -> Result<T, Error>;

    /// Similar to [ABCI CheckTx method](https://docs.tendermint.com/master/spec/abci/abci.html#checktx)
    /// > CheckTx need not execute the transaction in full, but rather a light-weight yet
    /// > stateful validation, like checking signatures and account balances, but not running
    /// > code in a virtual machine.
    fn check(&self, _message: Any) -> Result<(), Error> {
        Ok(())
    }

    /// Execute specified `Message`, modify state accordingly and return resulting `Events`
    /// Similar to [ABCI DeliverTx method](https://docs.tendermint.com/master/spec/abci/abci.html#delivertx)
    /// *NOTE* - Implementations MUST be deterministic!
    fn deliver(&mut self, message: Any) -> Result<Vec<Event>, Error>;

    /// Similar to [ABCI InitChain method](https://docs.tendermint.com/master/spec/abci/abci.html#initchain)
    /// Just as with `InitChain`, implementations are encouraged to panic on error
    fn init(&mut self, _app_state: serde_json::Value) {}

    /// Similar to [ABCI Query method](https://docs.tendermint.com/master/spec/abci/abci.html#query)
    fn query(&self, _data: &[u8], _path: &Path, _height: Height) -> Result<Vec<u8>, Error> {
        Err(Error::Unhandled)
    }
}

pub(crate) trait IdentifiableBy<I: Sized + Eq + Hash> {
    fn identifier(&self) -> I;
}

impl<S: Store> IdentifiableBy<&'static str> for Bank<S> {
    fn identifier(&self) -> &'static str {
        "bank"
    }
}

impl<S: Store> IdentifiableBy<&'static str> for Ibc<S> {
    fn identifier(&self) -> &'static str {
        "ibc"
    }
}

#[derive(Debug)]
pub enum Error {
    Unhandled,
    BankError(bank::Error),
    IbcError(ibc::Error),
}
