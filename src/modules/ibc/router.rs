use std::{borrow::Borrow, collections::BTreeMap, sync::Arc};

use ibc::core::ics26_routing::context::{Module as IbcModule, ModuleId};

#[derive(Clone, Default, Debug)]
pub struct IbcRouter(pub BTreeMap<ModuleId, Arc<dyn IbcModule>>);

impl IbcRouter {
    pub fn get_route(&self, module_id: &impl Borrow<ModuleId>) -> Option<&dyn IbcModule> {
        self.0.get(module_id.borrow()).map(Arc::as_ref)
    }

    pub fn get_route_mut(&mut self, module_id: &impl Borrow<ModuleId>) -> Option<&mut dyn IbcModule> {
        self.0.get_mut(module_id.borrow()).and_then(Arc::get_mut)
    }

    pub fn add_route(
        &mut self,
        module_id: ModuleId,
        module: impl IbcModule,
    ) -> Result<(), String> {
        match self.0.insert(module_id, Arc::new(module)) {
            None => Ok(()),
            Some(_) => Err("Duplicate module_id".to_owned()),
        }
    }
}
