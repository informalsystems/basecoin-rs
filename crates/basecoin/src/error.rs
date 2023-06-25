use cosmos_sdk_rs_bank::error::Error as BankError;
use cosmos_sdk_rs_gov::error::Error as GovError;
use cosmos_sdk_rs_helper::error::Error as HelperError;
use cosmos_sdk_rs_ibc::error::Error as IbcError;
use ibc::core::ContextError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no module could handle specified message")]
    NotHandled,
    #[error("custom error: `{reason}`")]
    Custom { reason: String },
    #[error("helper error: `{0}`")]
    Helper(HelperError),
    #[error("bank module error: `{0}`")]
    Bank(BankError),
    #[error("IBC module error: `{0}`")]
    Ibc(IbcError),
    #[error("Governance module error: `{0}`")]
    Gov(GovError),
}

impl From<ContextError> for Error {
    fn from(error: ContextError) -> Self {
        Self::Ibc(error.into())
    }
}
