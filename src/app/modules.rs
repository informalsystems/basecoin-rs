mod bank;
mod ibc;

pub(crate) use self::bank::Bank;
pub(crate) use self::ibc::Ibc;

use crate::app::store::{Height, Path};

use flex_error::{define_error, TraceError};
use prost_types::Any;
use std::fmt::Display;
use tendermint_proto::abci::Event;

define_error! {
    #[derive(PartialEq, Eq)]
    Error {
        Unhandled
            | _ | { "no module could handle specified message" },
        Bank
            [ TraceError<bank::Error> ]
            | _ | { "bank module error" },
        Ibc
            [ ibc::Error ]
            | _ | { "IBC module error" },
    }
}

pub(crate) trait Module {
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
        Err(Error::unhandled())
    }
}

pub(crate) trait Identifiable {
    type Identifier: Display;

    fn identifier(&self) -> Self::Identifier;
}

pub(crate) mod prefix {
    use super::Identifiable;

    #[derive(Clone)]
    pub(crate) struct Bank;

    impl Identifiable for Bank {
        type Identifier = &'static str;

        fn identifier(&self) -> &'static str {
            "bank"
        }
    }

    #[derive(Clone)]
    pub(crate) struct Ibc;

    impl Identifiable for Ibc {
        type Identifier = &'static str;

        fn identifier(&self) -> &'static str {
            "ibc"
        }
    }
}
