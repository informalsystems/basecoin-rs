mod auth;
mod bank;
mod ibc;
mod staking;

pub(crate) use self::auth::{Auth, ACCOUNT_PREFIX};
pub(crate) use self::bank::Bank;
pub(crate) use self::ibc::{Ibc, IbcRouterBuilder, IbcTransferModule};
pub(crate) use self::staking::Staking;

use crate::app::store::{self, Height, Path, SharedStore};

use cosmrs::AccountId;
use flex_error::{define_error, TraceError};
use ibc_proto::google::protobuf::Any;
use tendermint::block::Header;
use tendermint_proto::abci::Event;
use tendermint_proto::crypto::ProofOp;

define_error! {
    #[derive(PartialEq, Eq)]
    Error {
        NotHandled
            | _ | { "no module could handle specified message" },
        Custom
            { reason: String }
            | e | { format!("custom error: {0}", e.reason) },
        Store
            [ TraceError<store::Error> ]
            | _ | { "store error" },
        Bank
            [ TraceError<bank::Error> ]
            | _ | { "bank module error" },
        Ibc
            [ TraceError<ibc::Error> ]
            | _ | { "IBC module error" },
    }
}

pub(crate) trait Module: Send + Sync {
    /// The module's store type.
    type Store;

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
    ///
    /// ## Return
    /// * `Error::not_handled()` if message isn't known to OR hasn't been consumed (but possibly intercepted) by this module
    /// * Other errors iff message was meant to be consumed by module but resulted in an error
    /// * Resulting events on success
    fn deliver(&mut self, _message: Any, _signer: &AccountId) -> Result<Vec<Event>, Error> {
        Err(Error::not_handled())
    }

    /// Similar to [ABCI InitChain method](https://docs.tendermint.com/master/spec/abci/abci.html#initchain)
    /// Just as with `InitChain`, implementations are encouraged to panic on error
    fn init(&mut self, _app_state: serde_json::Value) {}

    /// Similar to [ABCI Query method](https://docs.tendermint.com/master/spec/abci/abci.html#query)
    ///
    /// ## Return
    /// * `Error::not_handled()` if message isn't known to OR hasn't been responded to (but possibly intercepted) by this module
    /// * Other errors iff query was meant to be consumed by module but resulted in an error
    /// * Query result  on success
    fn query(
        &self,
        _data: &[u8],
        _path: Option<&Path>,
        _height: Height,
        _prove: bool,
    ) -> Result<QueryResult, Error> {
        Err(Error::not_handled())
    }

    /// Similar to [ABCI BeginBlock method](https://docs.tendermint.com/master/spec/abci/abci.html#beginblock)
    /// *NOTE* - Implementations MUST be deterministic!
    ///
    /// ## Return
    /// * Resulting events if any
    fn begin_block(&mut self, _header: &Header) -> Vec<Event> {
        vec![]
    }

    /// Return a mutable reference to the module's store
    fn store_mut(&mut self) -> &mut SharedStore<Self::Store>;

    /// Return a reference to the module's store
    fn store(&self) -> &SharedStore<Self::Store>;
}

pub(crate) struct QueryResult {
    pub(crate) data: Vec<u8>,
    pub(crate) proof: Option<Vec<ProofOp>>,
}

/// Trait for identifying modules
/// This is used to get `Module` prefixes that are used for creating prefixed key-space proxy-stores
pub(crate) trait Identifiable {
    type Identifier: Into<store::Identifier>;

    /// Return an identifier
    fn identifier(&self) -> Self::Identifier;
}

pub(crate) mod prefix {
    use super::Identifiable;
    use crate::app::store;
    use core::convert::TryInto;

    /// Bank module prefix
    #[derive(Clone)]
    pub(crate) struct Bank;

    impl Identifiable for Bank {
        type Identifier = store::Identifier;

        fn identifier(&self) -> Self::Identifier {
            "bank".to_owned().try_into().unwrap()
        }
    }

    /// Ibc module prefix
    #[derive(Clone)]
    pub(crate) struct Ibc;

    impl Identifiable for Ibc {
        type Identifier = store::Identifier;

        fn identifier(&self) -> Self::Identifier {
            "ibc".to_owned().try_into().unwrap()
        }
    }

    /// Auth module prefix
    #[derive(Clone)]
    pub(crate) struct Auth;

    impl Identifiable for Auth {
        type Identifier = store::Identifier;

        fn identifier(&self) -> Self::Identifier {
            "auth".to_owned().try_into().unwrap()
        }
    }

    /// Staking module prefix
    #[derive(Clone)]
    pub(crate) struct Staking;

    impl Identifiable for Staking {
        type Identifier = store::Identifier;

        fn identifier(&self) -> Self::Identifier {
            "staking".to_owned().try_into().unwrap()
        }
    }
}
