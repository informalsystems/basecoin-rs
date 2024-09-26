use ibc::core::handler::types::error::HandlerError;

pub use crate::error::Error as AppError;

pub type Error = HandlerError;

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        AppError::Ibc(e)
    }
}
