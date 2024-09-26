/// Block height
pub type RawHeight = u64;

/// Store height to query
#[derive(Debug, Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub enum Height {
    Pending,
    Latest,
    Stable(RawHeight), // or equivalently `tendermint::block::Height`
}

impl From<RawHeight> for Height {
    fn from(value: u64) -> Self {
        match value {
            0 => Self::Latest, // see https://docs.tendermint.com/master/spec/abci/abci.html#query
            _ => Self::Stable(value),
        }
    }
}
