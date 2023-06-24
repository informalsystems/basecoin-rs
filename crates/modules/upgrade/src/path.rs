use derive_more::Display;

// Specifies the byte under which a pending upgrade plan is stored
const PLAN_BYTE: &[u8] = b"0x0";

// Specifies the byte under which a completed upgrade plan is stored
const DONE_BYTE: &[u8] = b"0x1";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
pub struct UpgradePlanPath(String);

impl UpgradePlanPath {
    pub fn new(path: String) -> Self {
        Self(path)
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(String::from_utf8(bytes.to_vec()).unwrap())
    }

    pub fn sdk_pending_path() -> Self {
        Self::from_bytes(PLAN_BYTE)
    }

    pub fn sdk_done_path() -> Self {
        Self::from_bytes(DONE_BYTE)
    }
}

impl From<UpgradePlanPath> for cosmos_sdk_rs_helper::Path {
    fn from(ibc_path: UpgradePlanPath) -> Self {
        Self::try_from(ibc_path.to_string()).unwrap() // safety - `IbcPath`s are correct-by-construction
    }
}
