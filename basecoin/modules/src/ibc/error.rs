use ibc::core::handler::types::error::ContextError;

pub use crate::error::Error as AppError;

pub type Error = ContextError;

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        AppError::Ibc(e)
    }
}
