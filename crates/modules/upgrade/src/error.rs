use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("unknown error: {0}")]
    Unknown(String),

    #[error("Invalid path")]
    InvalidPath,

    #[error("Proof not Found")]
    ProofNotFound,

    #[error("Data not Found")]
    DataNotFound,

    #[error("not handled")]
    NotHandled,
}
