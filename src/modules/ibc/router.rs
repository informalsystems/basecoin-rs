use std::{borrow::Borrow, collections::BTreeMap, sync::Arc};

use ibc::core::ics26_routing::context::{Module, ModuleId};

pub trait IbcModuleWrapper: Module + Send + Sync {
    fn as_ibc_module(&self) -> &dyn Module;
    fn as_ibc_module_mut(&mut self) -> &mut dyn Module;
}

#[derive(Clone, Default, Debug)]
pub struct IbcRouter(pub BTreeMap<ModuleId, Arc<dyn IbcModuleWrapper>>);

impl IbcRouter {
    pub fn get_route(&self, module_id: &impl Borrow<ModuleId>) -> Option<&dyn Module> {
        self.0
            .get(module_id.borrow())
            .map(|mod_wrapper| mod_wrapper.as_ibc_module())
    }

    pub fn get_route_mut(&mut self, module_id: &impl Borrow<ModuleId>) -> Option<&mut dyn Module> {
        self.0
            .get_mut(module_id.borrow())
            .and_then(Arc::get_mut)
            .map(|mod_wrapper| mod_wrapper.as_ibc_module_mut())
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
