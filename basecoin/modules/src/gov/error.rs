pub use displaydoc::Display;

pub use crate::error::Error as AppError;

#[derive(Debug, Display)]
pub enum Error {
    /// invalid proposal: `{reason}`
    InvalidProposal { reason: String },
    /// failed to validate: `{reason}`
    ValidationFailure { reason: String },
    /// failed to execute: `{reason}`
    ExecutionFailure { reason: String },
}

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        Self::Gov(e)
    }
}
