use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt::Debug;

use basecoin_store::context::Store;
use ibc::apps::transfer::types::MODULE_ID_STR as IBC_TRANSFER_MODULE_ID;
use ibc::core::host::types::error::HostError;
use ibc::core::host::types::identifiers::PortId;
use ibc::core::router::module::Module as IbcModule;
use ibc::core::router::router::Router;
use ibc::core::router::types::module::ModuleId;

use crate::bank::BankBalanceKeeper;
use crate::ibc::transfer::IbcTransferModule;

#[derive(Clone, Debug)]
pub struct IbcRouter<S>
where
    S: Store + Debug,
{
    transfer: IbcTransferModule<BankBalanceKeeper<S>>,

    /// Mapping of which IBC modules own which port
    port_to_module_map: BTreeMap<PortId, ModuleId>,
}

impl<S> IbcRouter<S>
where
    S: Store + Debug,
{
    pub fn new(transfer: IbcTransferModule<BankBalanceKeeper<S>>) -> Self {
        let mut port_to_module_map = BTreeMap::default();
        let transfer_module_id: ModuleId = ModuleId::new(IBC_TRANSFER_MODULE_ID.to_string());
        port_to_module_map.insert(PortId::transfer(), transfer_module_id);

        IbcRouter {
            transfer,
            port_to_module_map,
        }
    }

    pub fn transfer(self) -> IbcTransferModule<BankBalanceKeeper<S>> {
        self.transfer
    }
}

impl<S> Router for IbcRouter<S>
where
    S: Store + Debug,
{
    fn get_route(&self, module_id: &ModuleId) -> Option<&dyn IbcModule> {
        if <ModuleId as Borrow<str>>::borrow(module_id) == IBC_TRANSFER_MODULE_ID {
            Some(&self.transfer as &dyn IbcModule)
        } else {
            None
        }
    }

    fn get_route_mut(&mut self, module_id: &ModuleId) -> Option<&mut dyn IbcModule> {
        if <ModuleId as Borrow<str>>::borrow(module_id) == IBC_TRANSFER_MODULE_ID {
            Some(&mut self.transfer as &mut dyn IbcModule)
        } else {
            None
        }
    }

    fn lookup_module(&self, port_id: &PortId) -> Option<ModuleId> {
        self.port_to_module_map
            .get(port_id)
            .ok_or_else(|| HostError::missing_state(format!("missing port_id: {port_id}")))
            .cloned()
            .ok()
    }
}
