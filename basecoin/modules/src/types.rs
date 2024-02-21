use basecoin_store::impls::RevertibleStore;
use basecoin_store::types::Identifier;
use tendermint::merkle::proof::ProofOp;

use crate::context::Module;

pub type ModuleList<S> = Vec<IdentifiedModule<S>>;
pub type ModuleStore<S> = RevertibleStore<S>;

pub struct IdentifiedModule<S> {
    pub id: Identifier,
    pub module: Box<dyn Module<Store = ModuleStore<S>>>,
}

pub struct QueryResult {
    pub data: Vec<u8>,
    pub proof: Option<Vec<ProofOp>>,
}
