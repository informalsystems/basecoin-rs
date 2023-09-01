pub use crate::types::error::Error as AppError;
pub use displaydoc::Display;

#[derive(Debug, Display)]
pub enum Error {
    /// invalid proposal: `{reason}`
    InvalidProposal { reason: String },
}

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        AppError::Gov(e)
    }
}
