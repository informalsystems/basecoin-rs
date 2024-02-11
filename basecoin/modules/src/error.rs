use displaydoc::Display;

use crate::bank::Error as BankError;
use crate::gov::Error as GovError;
use crate::ibc::Error as IbcError;

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
