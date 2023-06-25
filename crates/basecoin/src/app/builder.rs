use std::sync::{Arc, RwLock};
use tracing::error;

use crate::error::Error;
use cosmrs::AccountId;
use ibc_proto::google::protobuf::Any;
use tendermint_proto::abci::Event;

use cosmos_sdk_rs_helper::Identifier;
use cosmos_sdk_rs_module_api::types::IdentifiedModule;
use cosmos_sdk_rs_module_api::types::ModuleList;
use cosmos_sdk_rs_module_api::types::ModuleStore;

use cosmos_sdk_rs_module_api::module::Module;

use cosmos_sdk_rs_store::{MainStore, ProvableStore, RevertibleStore, SharedRw, SharedStore};

pub struct Builder<S> {
    store: MainStore<S>,
    modules: SharedRw<ModuleList<S>>,
}

impl<S: Default + ProvableStore + 'static> Builder<S> {
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
        let modules = self.modules.read().unwrap();
        modules
            .iter()
            .find(|m| &m.id == prefix)
            .map(|IdentifiedModule { module, .. }| module.store().share())
            .unwrap_or_else(|| SharedStore::new(ModuleStore::new(S::default())))
    }

    #[inline]
    fn is_unique_id(&self, prefix: &Identifier) -> bool {
        !self.modules.read().unwrap().iter().any(|m| &m.id == prefix)
    }

    /// Adds a new module. Panics if a module with the specified identifier was previously added.
    pub fn add_module(
        self,
        prefix: Identifier,
        module: impl Module<Store = ModuleStore<S>> + 'static,
    ) -> Self {
        assert!(self.is_unique_id(&prefix), "module prefix must be unique");
        self.modules.write().unwrap().push(IdentifiedModule {
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

impl<S: Default + ProvableStore> BaseCoinApp<S> {
    // try to deliver the message to all registered modules
    // if `module.deliver()` returns `Error::NotHandled`, try next module
    // Return:
    // * other errors immediately OR
    // * `Error::NotHandled` if all modules return `Error::NotHandled`
    // * events from first successful deliver call
    pub fn deliver_msg(&self, message: Any, signer: &AccountId) -> Result<Vec<Event>, Error> {
        let mut modules = self.modules.write().unwrap();
        let mut handled = false;
        let mut events = vec![];

        for IdentifiedModule { module, .. } in modules.iter_mut() {
            match module.deliver(message.clone(), signer) {
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                    handled = true;
                    break;
                }
                // todo(davirain)
                Err(e) if e.to_string() == anyhow::anyhow!(Error::NotHandled).to_string() => {
                    continue
                }
                Err(e) => {
                    error!("deliver message ({:?}) failed with error: {:?}", message, e);
                    return Err(Error::Custom {
                        reason: e.to_string(),
                    });
                }
            }
        }
        if handled {
            Ok(events)
        } else {
            Err(Error::NotHandled)
        }
    }
}
