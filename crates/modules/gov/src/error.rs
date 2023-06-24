pub use displaydoc::Display;

#[derive(Debug, Display)]
pub enum Error {
    /// invalid proposal: `{reason}`
    InvalidProposal { reason: String },
}
