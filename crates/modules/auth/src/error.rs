use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("unknown error: {0}")]
    Unknown(String),

    #[error("unknown signer")]
    UnknownSigner,

    #[error("failed to increment signer sequence")]
    FailedToIncrementSignerSequence,

    #[error("Invalid path")]
    InvalidPath,

    #[error("Proof not Found")]
    ProofNotFound,

    #[error("Data not Found")]
    DataNotFound,

    #[error("not handled")]
    NotHandled,
}
