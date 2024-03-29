use core::fmt::{Debug, Display, Formatter};
use core::ops::Deref;

/// A new type representing a valid ICS024 identifier.
/// Implements `Deref<Target=String>`.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Identifier(String);

impl Deref for Identifier {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for Identifier {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
