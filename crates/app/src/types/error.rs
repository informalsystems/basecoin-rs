use crate::modules::bank::error::Error as BankError;
use crate::modules::gov::error::Error as GovError;
use crate::modules::ibc::error::Error as IbcError;
use displaydoc::Display;
use ibc::core::ContextError;

#[derive(Debug, Display)]
pub enum Error {
    /// no module could handle specified message
    NotHandled,
    /// custom error: `{reason}`
    Custom { reason: String },
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
