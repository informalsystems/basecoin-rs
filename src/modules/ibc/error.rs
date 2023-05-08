pub use crate::error::Error as AppError;
use ibc::core::RouterError;

pub type Error = RouterError;

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        AppError::Ibc(e)
    }
}
