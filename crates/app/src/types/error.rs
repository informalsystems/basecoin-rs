use crate::modules::bank::error::Error as BankError;
use crate::modules::gov::error::Error as GovError;
use crate::modules::ibc::error::Error as IbcError;
use displaydoc::Display;

#[derive(Debug, Display)]
pub enum Error {
    /// no module could handle specified message
    NotHandled,
    /// custom error: `{reason}`
    Custom { reason: String },
    /// bank module error: `{0}`
    Bank(BankError),
    /// IBC module error: `{0}`
    Ibc(IbcError),
    /// Governance module error: `{0}`
    Gov(GovError),
}
