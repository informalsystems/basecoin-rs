use std::{borrow::Borrow, collections::BTreeMap, sync::Arc};

use ibc::core::ics26_routing::context::{Module as IbcModule, ModuleId};

#[derive(Clone, Default, Debug)]
pub struct IbcRouter(pub BTreeMap<ModuleId, Arc<dyn IbcModule + Send + Sync>>);

impl IbcRouter {
    pub fn get_route(&self, module_id: &impl Borrow<ModuleId>) -> Option<&dyn IbcModule> {
        self.0
            .get(module_id.borrow())
            .map(|x| Arc::as_ref(x) as &dyn IbcModule)
    }

    pub fn get_route_mut(
        &mut self,
        module_id: &impl Borrow<ModuleId>,
    ) -> Option<&mut dyn IbcModule> {
        // we can't write:
        // self.0.get_mut(module_id.borrow()).and_then(Arc::get_mut)
        // due to a compiler bug

        match self.0.get_mut(module_id.borrow()) {
            Some(arc_mod) => match Arc::get_mut(arc_mod) {
                Some(m) => Some(m as &mut dyn IbcModule),
                None => None,
            },
            None => None,
        }
    }

    pub fn add_route(
        &mut self,
        module_id: ModuleId,
        module: Arc<dyn IbcModule + Send + Sync>,
    ) -> Result<(), String> {
        match self.0.insert(module_id, module) {
            None => Ok(()),
            Some(_) => Err("Duplicate module_id".to_owned()),
        }
    }
}
