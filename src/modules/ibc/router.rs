use core::fmt::Debug;
use std::{any::Any, borrow::Borrow, collections::BTreeMap, sync::Arc};

use ibc::core::ics26_routing::context::{Module, ModuleId};

// Trait to be implemented on all concrete structs that implement `Module`
pub trait IbcModuleWrapper: Debug + Send + Sync + Module + Any {
    fn as_ibc_module(&self) -> &(dyn Module + 'static);
    fn as_ibc_module_mut(&mut self) -> &mut (dyn Module + 'static);
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

#[derive(Clone, Default, Debug)]
pub struct IbcRouter(pub BTreeMap<ModuleId, Arc<dyn IbcModuleWrapper + 'static>>);

impl IbcRouter {
    pub fn get_route(&self, module_id: &impl Borrow<ModuleId>) -> Option<&(dyn IbcModuleWrapper + 'static)> {
        self.0.get(module_id.borrow()).map(Arc::as_ref)
    }

    pub fn get_route_mut(
        &mut self,
        module_id: &impl Borrow<ModuleId>,
    ) -> Option<&mut dyn IbcModuleWrapper> {
        self.0.get_mut(module_id.borrow()).and_then(Arc::get_mut)
    }

    pub fn add_route(
        &mut self,
        module_id: ModuleId,
        module: impl IbcModuleWrapper,
    ) -> Result<(), String> {
        match self.0.insert(module_id, Arc::new(module)) {
            None => Ok(()),
            Some(_) => Err("Duplicate module_id".to_owned()),
        }
    }
}
