use crate::module::Module;
use cosmos_sdk_rs_helper::Identifier;
use cosmos_sdk_rs_store::RevertibleStore;

pub type ModuleList<S> = Vec<IdentifiedModule<S>>;
pub type ModuleStore<S> = RevertibleStore<S>;

pub struct IdentifiedModule<S> {
    pub id: Identifier,
    pub module: Box<dyn Module<Store = ModuleStore<S>>>,
}
