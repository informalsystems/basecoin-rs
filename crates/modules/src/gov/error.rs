pub use displaydoc::Display;

pub use crate::types::Error as AppError;

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
