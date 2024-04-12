pub use displaydoc::Display;

pub use crate::error::Error as AppError;

#[derive(Debug, Display)]
pub enum Error {
    /// invalid proposal: `{reason}`
    InvalidProposal { reason: String },
    /// an error occurred while encoding proposal: `{reason}`
    Encoding { reason: String },
    /// a client error occurred: `{reason}`
    Client { reason: String },
}

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        AppError::Gov(e)
    }
}
