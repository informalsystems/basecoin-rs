use ibc::core::ContextError;
use ibc::core::RouterError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("unknown error: {0}")]
    Unknown(String),

    #[error("ibc router error: {0}")]
    RouterError(#[from] RouterError),

    #[error("ibc context error: {0}")]
    ContextError(#[from] ContextError),

    #[error("Invalid domain path:({0})")]
    InvalidDomainPath(String),

    #[error("Invalid IBC path:({0})")]
    InvalidIbcPath(String),

    #[error("Proof not Found")]
    ProofNotFound,

    #[error("Data not Found")]
    DataNotFound,

    #[error("not handled")]
    NotHandled,
}
