pub use crate::error::Error as AppError;
pub use ibc::core::ics26_routing::error::RouterError;
pub type Error = RouterError;

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        AppError::Ibc(e)
    }
}
