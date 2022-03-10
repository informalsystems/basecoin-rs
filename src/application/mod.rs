//! The basecoin ABCI application.
mod abci;
mod grpc;
mod response;

use crate::modules::{Error, ErrorDetail, Module};
use crate::store::{Identifier, ProvableStore, RevertibleStore, SharedStore};

use std::sync::{Arc, RwLock};

use prost_types::Any;
use tendermint_proto::abci::Event;

type MainStore<S> = SharedStore<RevertibleStore<S>>;
type ModuleStore<S> = RevertibleStore<S>;
type ModuleList<S> = Vec<(Identifier, Box<dyn Module<ModuleStore<S>>>)>;
type Shared<T> = Arc<RwLock<T>>;

/// BaseCoin ABCI application.
///
/// Can be safely cloned and sent across threads, but not shared.
#[derive(Clone)]
pub struct Application<S> {
    store: MainStore<S>,
    modules: Shared<ModuleList<S>>,
}

impl<S: Default + ProvableStore + 'static> Application<S> {
    /// Constructor.
    pub fn new(store: S) -> Result<Self, S::Error> {
        Ok(Self {
            store: SharedStore::new(RevertibleStore::new(store)),
            modules: Arc::new(RwLock::new(vec![])),
        })
    }

    #[inline]
    fn is_unique_id(&self, prefix: &Identifier) -> bool {
        !self
            .modules
            .read()
            .unwrap()
            .iter()
            .any(|(id, _)| id == prefix)
    }

    pub fn add_module(
        self,
        prefix: Identifier,
        module: impl Module<ModuleStore<S>> + 'static,
    ) -> Self {
        assert!(self.is_unique_id(&prefix), "module prefix must be unique");
        self.modules
            .write()
            .unwrap()
            .push((prefix, Box::new(module)));
        self
    }
}

impl<S: Default + ProvableStore> Application<S> {
    pub fn module_store(&self, prefix: &Identifier) -> SharedStore<ModuleStore<S>> {
        let modules = self.modules.read().unwrap();
        modules
            .iter()
            .find(|(p, _)| p == prefix)
            .map(|(_, m)| m.store().clone())
            .unwrap_or_else(|| SharedStore::new(ModuleStore::new(S::default())))
    }

    // try to deliver the message to all registered modules
    // if `module.deliver()` returns `Error::not_handled()`, try next module
    // Return:
    // * other errors immediately OR
    // * `Error::not_handled()` if all modules return `Error::not_handled()`
    // * events from first successful deliver call OR
    fn deliver_msg(&self, message: Any) -> Result<Vec<Event>, Error> {
        let mut modules = self.modules.write().unwrap();
        let mut handled = false;
        let mut events = vec![];

        for (_, m) in modules.iter_mut() {
            match m.deliver(message.clone()) {
                Ok(mut msg_events) => {
                    events.append(&mut msg_events);
                    handled = true;
                    break;
                }
                Err(Error(ErrorDetail::NotHandled(_), _)) => continue,
                Err(e) => return Err(e),
            };
        }

        if handled {
            Ok(events)
        } else {
            Err(Error::not_handled())
        }
    }
}
