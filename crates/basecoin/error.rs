use super::helper::error::Error as HelperError;
use super::modules::bank::error::Error as BankError;
use super::modules::ibc::error::Error as IbcError;
use crate::modules::gov::error::Error as GovError;
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
