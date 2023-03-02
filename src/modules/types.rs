use super::Module;
use crate::{helper::Identifier, store::RevertibleStore};

pub(crate) type ModuleList<S> = Vec<IdentifiedModule<S>>;
pub(crate) type ModuleStore<S> = RevertibleStore<S>;

pub struct IdentifiedModule<S> {
    pub id: Identifier,
    pub module: Box<dyn Module<Store = ModuleStore<S>>>,
}
