use tendermint::merkle::proof::ProofOp;

pub struct QueryResult {
    pub data: Vec<u8>,
    pub proof: Option<Vec<ProofOp>>,
}
