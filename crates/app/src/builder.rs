use crate::modules::context::prefix;
use crate::modules::context::Identifiable;
use crate::modules::context::Module;
use crate::modules::types::IdentifiedModule;
use crate::modules::types::ModuleList;
use crate::modules::types::ModuleStore;
use crate::modules::Ibc;
use crate::types::error::Error;

use basecoin_store::context::ProvableStore;
use basecoin_store::impls::RevertibleStore;
use basecoin_store::impls::SharedStore;
use basecoin_store::types::Identifier;
use basecoin_store::types::MainStore;
use basecoin_store::utils::{SharedRw, SharedRwExt};

use cosmrs::AccountId;
use ibc_proto::google::protobuf::Any;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};
use tendermint::abci::Event;
use tracing::error;

pub struct Builder<S> {
    store: MainStore<S>,
    modules: SharedRw<ModuleList<S>>,
}

impl<S: Default + ProvableStore> Builder<S> {
    /// Constructor.
    pub fn new(store: S) -> Self {
        Self {
            store: SharedStore::new(RevertibleStore::new(store)),
            modules: Arc::new(RwLock::new(vec![])),
        }
    }

    /// Returns a share to the module's store if a module with specified identifier was previously
    /// added, otherwise creates a new module store and returns it.
    pub fn module_store(&self, prefix: &Identifier) -> SharedStore<ModuleStore<S>> {
        let modules = self.modules.read_access();
        modules
            .iter()
            .find(|m| &m.id == prefix)
            .map(|IdentifiedModule { module, .. }| module.store().share())
            .unwrap_or_else(|| SharedStore::new(ModuleStore::new(S::default())))
    }

    #[inline]
    fn is_unique_id(&self, prefix: &Identifier) -> bool {
        !self.modules.read_access().iter().any(|m| &m.id == prefix)
    }

    /// Adds a new module. Panics if a module with the specified identifier was previously added.
    pub fn add_module(
        self,
        prefix: Identifier,
        module: impl Module<Store = ModuleStore<S>> + 'static,
    ) -> Self {
        assert!(self.is_unique_id(&prefix), "module prefix must be unique");
        self.modules.write_access().push(IdentifiedModule {
            id: prefix,
            module: Box::new(module),
        });
        self
    }

    pub fn build(self) -> BaseCoinApp<S> {
        BaseCoinApp {
            store: self.store,
            modules: self.modules,
        }
    }
}

/// BaseCoin ABCI application.
///
/// Can be safely cloned and sent across threads, but not shared.
#[derive(Clone)]
pub struct BaseCoinApp<S> {
    pub store: MainStore<S>,
    pub modules: SharedRw<ModuleList<S>>,
}

impl<S: Default + Debug + ProvableStore> BaseCoinApp<S> {
    // try to deliver the message to all registered modules
    // if `module.deliver()` returns `Error::NotHandled`, try next module
    // Return:
    // * other errors immediately OR
    // * `Error::NotHandled` if all modules return `Error::NotHandled`
    // * events from first successful deliver call
    pub fn deliver_msg(&self, message: Any, signer: &AccountId) -> Result<Vec<Event>, Error> {
        let mut modules = self.modules.write_access();
        let mut handled = false;
        let mut events = vec![];

        for IdentifiedModule { module, .. } in modules.iter_mut() {
            match module.deliver(message.clone(), signer) {
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                    handled = true;
                    break;
                }
                Err(Error::NotHandled) => continue,
                Err(e) => {
                    error!("deliver message ({:?}) failed with error: {:?}", message, e);
                    return Err(e);
                }
            }
        }
        if handled {
            Ok(events)
        } else {
            Err(Error::NotHandled)
        }
    }

    /// Gives access to the IBC module.
    pub fn ibc(&self) -> Ibc<RevertibleStore<S>> {
        let modules = self.modules.read_access();

        let ibc_module = modules
            .iter()
            .find(|m| m.id == prefix::Ibc {}.identifier())
            .and_then(|m| {
                let a = m
                    .module
                    .as_any()
                    .downcast_ref::<Ibc<RevertibleStore<S>>>()
                    .cloned();
                a
            })
            .expect("IBC module not found");

        ibc_module
    }
}
