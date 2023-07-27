use std::{borrow::Borrow, fmt::Debug, sync::Arc, collections::BTreeMap};

use ibc::{
    applications::transfer::MODULE_ID_STR as IBC_TRANSFER_MODULE_ID,
    core::{router::{Module as IbcModule, ModuleId, Router}, ics24_host::identifier::PortId, ics04_channel::error::PortError},
};

use crate::{
    modules::{bank::impls::BankBalanceKeeper, IbcTransferModule},
    store::Store,
};

#[derive(Clone, Debug)]
pub struct IbcRouter<S>
where
    S: Store + Send + Sync + Debug,
{
    transfer: Arc<IbcTransferModule<S, BankBalanceKeeper<S>>>,

    /// Mapping of which IBC modules own which port
    port_to_module_map: BTreeMap<PortId, ModuleId>,
}

impl<S> IbcRouter<S>
where
    S: 'static + Store + Send + Sync + Debug,
{
    pub fn new(transfer: IbcTransferModule<S, BankBalanceKeeper<S>>) -> Self {
        let mut port_to_module_map = BTreeMap::default();
        let transfer_module_id: ModuleId = ModuleId::new(IBC_TRANSFER_MODULE_ID.to_string());
        port_to_module_map.insert(PortId::transfer(), transfer_module_id);

        IbcRouter {
            transfer: Arc::new(transfer),
            port_to_module_map: BTreeMap::new(),
        }
    }

    pub fn get_transfer_module_mut(
        &mut self,
    ) -> Option<&mut IbcTransferModule<S, BankBalanceKeeper<S>>> {
        match Arc::get_mut(&mut self.transfer) {
            Some(m) => Some(m),
            None => None,
        }
    }
}

impl<S> Router for IbcRouter<S>
where
    S: 'static + Store + Send + Sync + Debug,
{
    fn get_route(&self, module_id: &ModuleId) -> Option<&dyn IbcModule> {
        if <ModuleId as Borrow<str>>::borrow(module_id) == IBC_TRANSFER_MODULE_ID {
            Some(Arc::as_ref(&self.transfer) as &dyn IbcModule)
        } else {
            None
        }
    }

    fn get_route_mut(&mut self, module_id: &ModuleId) -> Option<&mut dyn IbcModule> {
        if <ModuleId as Borrow<str>>::borrow(module_id) == IBC_TRANSFER_MODULE_ID {
            match Arc::get_mut(&mut self.transfer) {
                Some(m) => Some(m),
                None => None,
            }
        } else {
            None
        }
    }

    fn lookup_module_by_port(&self, port_id: &PortId) -> Option<ModuleId> {
        self.port_to_module_map
            .get(port_id)
            .ok_or(PortError::UnknownPort {
                port_id: port_id.clone(),
            })
            .map(Clone::clone)
            .ok()
    }
}
