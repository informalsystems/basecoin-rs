pub mod bank;
pub mod ibc;

use crate::app::modules::bank::Bank;
use crate::app::store::{Height, Path, Store};
use prost::Message;
use prost_types::Any;
use serde::de::Deserialize;
use std::hash::Hash;
use tendermint_proto::abci::Event;

pub trait Module<S: Store> {
    /// Tries to decode a protobuf message to a module supported Message`
    /// This is used to determine if a message is handleable by this module or not
    /// Do NOT use for validation!
    fn decode<T: Message + Default>(message: Any) -> Result<T, Error>;

    /// Similar to [ABCI CheckTx method](https://docs.tendermint.com/master/spec/abci/abci.html#checktx)
    /// > CheckTx need not execute the transaction in full, but rather a light-weight yet
    /// > stateful validation, like checking signatures and account balances, but not running
    /// > code in a virtual machine.
    fn check(_store: &S, message: Any) -> Result<(), Error> {
        let _ = Self::decode(message)?;
        Ok(())
    }

    /// Execute specified `Message`, modify state accordingly and return resulting `Events`
    /// Similar to [ABCI DeliverTx method](https://docs.tendermint.com/master/spec/abci/abci.html#delivertx)
    /// *NOTE* - Implementations MUST be deterministic!
    fn deliver(store: &mut S, message: Any) -> Result<Vec<Event>, Error>;

    /// Similar to [ABCI InitChain method](https://docs.tendermint.com/master/spec/abci/abci.html#initchain)
    /// Just as with `InitChain`, implementations are encouraged to panic on error
    fn init(_store: &mut S, _app_state: serde_json::Value) {}

    /// Similar to [ABCI Query method](https://docs.tendermint.com/master/spec/abci/abci.html#query)
    fn query<'de, T: Deserialize<'de>>(
        _store: &S,
        _data: impl AsRef<u8>,
        _path: &Path,
        _height: Height,
    ) -> Result<T, Error> {
        unimplemented!()
    }
}

pub(crate) trait IdentifiableBy<I: Sized + Eq + Hash> {
    fn identifier() -> I;
}

impl IdentifiableBy<&'static str> for Bank {
    fn identifier() -> &'static str {
        "bank"
    }
}

pub enum Error {
    Unhandled,
    BankError(bank::Error),
}
