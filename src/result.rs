//! Application-specific `Result` type.

/// Type alias for results produced by functions in the basecoin application.
pub type Result<T> = std::result::Result<T, flex_error::DefaultTracer>;
