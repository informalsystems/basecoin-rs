use crate::modules::context::Module;
use basecoin_store::impls::RevertibleStore;
use basecoin_store::types::Identifier;

pub(crate) type ModuleList<S> = Vec<IdentifiedModule<S>>;
pub(crate) type ModuleStore<S> = RevertibleStore<S>;

pub struct IdentifiedModule<S> {
    pub id: Identifier,
    pub module: Box<dyn Module<Store = ModuleStore<S>>>,
}
