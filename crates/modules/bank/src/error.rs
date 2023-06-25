use cosmrs::AccountId;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to decode message")]
    MsgDecodeFailure,

    #[error("failed to validate message: `{reason}`")]
    MsgValidationFailure { reason: String },

    #[error("account `{account}` doesn't exist")]
    NonExistentAccount { account: AccountId },

    #[error("insufficient funds in sender account")]
    InsufficientSourceFunds,

    #[error("receiver account funds overflow")]
    DestFundOverflow,

    #[error("Store error: `{reason}`")]
    Store { reason: String },

    #[error("not handled")]
    NotHandled,
}
