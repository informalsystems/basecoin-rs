use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("custom error: `{0}`")]
    Custom(String),

    #[error("Unknown type url: `{0}`")]
    UnknownTypeUrl(String),

    #[error("invalid proposal: `{reason}`")]
    InvalidProposal { reason: String },

    #[error("data not found")]
    DataNotFound,

    #[error("not handled")]
    NotHandled,
}
