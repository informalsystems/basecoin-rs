use ibc::core::ics24_host::identifier::IdentifierError;
use std::str::Utf8Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("'{identifier}' is not a valid identifier: `{error}`")]
    InvalidIdentifier {
        identifier: String,
        error: IdentifierError,
    },

    #[error("path isn't a valid string: `{error}`")]
    MalformedPathString { error: Utf8Error },

    #[error("Other: `{reason}`")]
    Other { reason: String },
}
