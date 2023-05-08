use std::{borrow::Borrow, fmt::Debug, sync::Arc};

use ibc::{
    applications::transfer::MODULE_ID_STR as IBC_TRANSFER_MODULE_ID,
    core::router::{Module as IbcModule, ModuleId},
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
}

impl<S> IbcRouter<S>
where
    S: 'static + Store + Send + Sync + Debug,
{
    pub fn new(transfer: IbcTransferModule<S, BankBalanceKeeper<S>>) -> Self {
        IbcRouter {
            transfer: Arc::new(transfer),
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

    pub fn get_route(&self, module_id: &ModuleId) -> Option<&dyn IbcModule> {
        if <ModuleId as Borrow<str>>::borrow(module_id) == IBC_TRANSFER_MODULE_ID {
            Some(Arc::as_ref(&self.transfer) as &dyn IbcModule)
        } else {
            None
        }
    }

    pub fn get_route_mut(&mut self, module_id: &ModuleId) -> Option<&mut dyn IbcModule> {
        if <ModuleId as Borrow<str>>::borrow(module_id) == IBC_TRANSFER_MODULE_ID {
            match Arc::get_mut(&mut self.transfer) {
                Some(m) => Some(m),
                None => None,
            }
        } else {
            None
        }
    }
}
