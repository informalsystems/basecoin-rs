// use crate::error::Error as AppError;
use displaydoc::Display;
use ibc::core::ics24_host::identifier::IdentifierError;
use std::str::Utf8Error;

#[derive(Debug, Display)]
pub enum Error {
    /// '{identifier}' is not a valid identifier: `{error}`
    InvalidIdentifier {
        identifier: String,
        error: IdentifierError,
    },
    /// path isn't a valid string: `{error}`
    MalformedPathString { error: Utf8Error },

    /// Other: `{reason}`
    Other { reason: String },
}

// impl From<Error> for AppError {
//     fn from(e: Error) -> Self {
//         AppError::Helper(e)
//     }
// }
