use basecoin_store::types::Path;
use derive_more::Display;

// Specifies the byte under which a proposal is stored
const PROPOSAL_BYTE: &[u8] = b"0x0";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
pub struct ProposalPath(String);

impl ProposalPath {
    pub fn new(path: String) -> Self {
        Self(path)
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(String::from_utf8(bytes.to_vec()).unwrap())
    }

    pub fn sdk_path() -> Self {
        Self::from_bytes(PROPOSAL_BYTE)
    }
}

impl From<ProposalPath> for Path {
    fn from(value: ProposalPath) -> Self {
        Self::try_from(value.to_string()).unwrap()
    }
}
