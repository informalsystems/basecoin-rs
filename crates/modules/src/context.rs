use std::any::Any as StdAny;

use basecoin_store::impls::SharedStore;
use basecoin_store::types::{Height, Identifier as StoreIdentifier, Path};
use cosmrs::AccountId;
use ibc_proto::google::protobuf::Any;
use tendermint::abci::Event;
use tendermint::block::Header;

use crate::types::{Error, QueryResult};

pub trait Module: Send + Sync + AsAny {
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
    /// * `Error::NotHandled` if message isn't known to OR hasn't been consumed (but possibly intercepted) by this module
    /// * Other errors iff message was meant to be consumed by module but resulted in an error
    /// * Resulting events on success
    fn deliver(&mut self, _message: Any, _signer: &AccountId) -> Result<Vec<Event>, Error> {
        Err(Error::NotHandled)
    }

    /// Similar to [ABCI InitChain method](https://docs.tendermint.com/master/spec/abci/abci.html#initchain)
    /// Just as with `InitChain`, implementations are encouraged to panic on error
    fn init(&mut self, _app_state: serde_json::Value) {}

    /// Similar to [ABCI Query method](https://docs.tendermint.com/master/spec/abci/abci.html#query)
    ///
    /// ## Return
    /// * `Error::NotHandled` if message isn't known to OR hasn't been responded to (but possibly intercepted) by this module
    /// * Other errors iff query was meant to be consumed by module but resulted in an error
    /// * Query result  on success
    fn query(
        &self,
        _data: &[u8],
        _path: Option<&Path>,
        _height: Height,
        _prove: bool,
    ) -> Result<QueryResult, Error> {
        Err(Error::NotHandled)
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

pub trait AsAny: StdAny {
    fn as_any(&self) -> &dyn StdAny;
}

impl<M: Module> AsAny for M {
    fn as_any(&self) -> &dyn StdAny {
        self
    }
}

/// Trait for identifying modules
/// This is used to get `Module` prefixes that are used for creating prefixed key-space proxy-stores
pub trait Identifiable {
    type Identifier: Into<StoreIdentifier>;

    /// Return an identifier
    fn identifier(&self) -> Self::Identifier;
}

pub mod prefix {
    use basecoin_store::types::Identifier as StoreIdentifier;

    use super::Identifiable;

    /// Bank module prefix
    #[derive(Clone)]
    pub struct Bank;

    impl Identifiable for Bank {
        type Identifier = StoreIdentifier;

        fn identifier(&self) -> Self::Identifier {
            "bank".to_owned().into()
        }
    }

    /// Ibc module prefix
    #[derive(Clone)]
    pub struct Ibc;

    impl Identifiable for Ibc {
        type Identifier = StoreIdentifier;

        fn identifier(&self) -> Self::Identifier {
            "ibc".to_owned().into()
        }
    }

    /// Auth module prefix
    #[derive(Clone)]
    pub struct Auth;

    impl Identifiable for Auth {
        type Identifier = StoreIdentifier;

        fn identifier(&self) -> Self::Identifier {
            "auth".to_owned().into()
        }
    }

    /// Governance module prefix
    #[derive(Clone)]
    pub struct Governance;

    impl Identifiable for Governance {
        type Identifier = StoreIdentifier;

        fn identifier(&self) -> Self::Identifier {
            "gov".to_owned().into()
        }
    }

    /// Staking module prefix
    #[derive(Clone)]
    pub struct Staking;

    impl Identifiable for Staking {
        type Identifier = StoreIdentifier;

        fn identifier(&self) -> Self::Identifier {
            "staking".to_owned().into()
        }
    }

    /// Governance module prefix
    #[derive(Clone)]
    pub struct Upgrade;

    impl Identifiable for Upgrade {
        type Identifier = StoreIdentifier;

        fn identifier(&self) -> Self::Identifier {
            "upgrade".to_owned().into()
        }
    }
}
