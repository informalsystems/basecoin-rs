pub mod ibc;

use crate::app::store::{Height, Path, Store};
use core::convert::{TryFrom, TryInto};
use prost_types::Any;
use serde::de::Deserialize;
use std::error::Error as StdError;
use tendermint_proto::abci::Event as AbciEvent;

pub trait Module<S: Store> {
    /// Error type - expected to envelop all possible errors in the module
    type Error: StdError;

    /// Event type - that can be converted to an `AbciEvent`
    type Event: Into<AbciEvent>;

    /// Message type - expected to envelope all messages that the module can handle
    /// Must be instantiatable from a corresponding and supported Protobuf message
    type Message: TryFrom<Any, Error = Self::Error>;

    /// Tries to decode a protobuf message to a module supported Message`
    /// This is used to determine if a message is handleable by this module or not
    /// Do NOT use for validation!
    fn decode(message: Any) -> Result<Self::Message, Self::Error> {
        message.try_into()
    }

    /// Similar to [ABCI CheckTx method](https://docs.tendermint.com/master/spec/abci/abci.html#checktx)
    /// > CheckTx need not execute the transaction in full, but rather a light-weight yet
    /// > stateful validation, like checking signatures and account balances, but not running
    /// > code in a virtual machine.
    fn check(_store: &S, message: Any) -> Result<(), Self::Error> {
        let _ = Self::decode(message)?;
        Ok(())
    }

    /// Execute specified `Message`, modify state accordingly and return resulting `Events`
    /// *NOTE* - Implementations MUST be deterministic!
    fn dispatch(store: &mut S, message: Self::Message) -> Result<Vec<Self::Event>, Self::Error>;

    /// Similar to [ABCI DeliverTx method](https://docs.tendermint.com/master/spec/abci/abci.html#delivertx)
    fn deliver(store: &mut S, message: Any) -> Result<Vec<Self::Event>, Self::Error> {
        let message = Self::decode(message)?;
        Self::dispatch(store, message)
    }

    /// Similar to [ABCI InitChain method](https://docs.tendermint.com/master/spec/abci/abci.html#initchain)
    /// Just as with `InitChain`, implementations are encouraged to panic on error
    fn init(_store: &mut S, _app_state: serde_json::Value) {}

    /// Similar to [ABCI Query method](https://docs.tendermint.com/master/spec/abci/abci.html#query)
    fn query<'de, T: Deserialize<'de>>(
        _store: &S,
        _data: impl AsRef<u8>,
        _path: &Path,
        _height: Height,
    ) -> Result<T, Self::Error> {
        unimplemented!()
    }
}
