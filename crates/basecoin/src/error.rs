use cosmos_sdk_rs_helper::error::Error as HelperError;
use cosmos_sdk_rs_bank::error::Error as BankError;
use cosmos_sdk_rs_ibc::error::Error as IbcError;
use cosmos_sdk_rs_gov::error::Error as GovError;
use displaydoc::Display;
use ibc::core::ContextError;

#[derive(Debug, Display)]
pub enum Error {
    /// no module could handle specified message
    NotHandled,
    /// custom error: `{reason}`
    Custom { reason: String },
    /// helper error
    Helper(HelperError),
    /// bank module error
    Bank(BankError),
    /// IBC module error
    Ibc(IbcError),
    /// Governance module error
    Gov(GovError),
}

impl From<ContextError> for Error {
    fn from(error: ContextError) -> Self {
        Self::Ibc(error.into())
    }
}
