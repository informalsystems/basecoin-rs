use core::fmt::{Display, Formatter};
use core::str::{from_utf8, FromStr, Utf8Error};

use displaydoc::Display as DisplayDoc;

use super::Identifier;
use crate::avl::{AsBytes, ByteSlice};

#[derive(Debug, DisplayDoc)]
pub enum Error {
    /// path isn't a valid string: `{error}`
    MalformedPathString { error: Utf8Error },
    /// parse error: `{0}`
    ParseError(String),
}

/// A new type representing a valid ICS024 `Path`.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Path(Vec<Identifier>);

impl Path {
    pub fn get(&self, index: usize) -> Option<&Identifier> {
        self.0.get(index)
    }

    pub fn try_into<K, E>(self) -> Result<K, E>
    where
        K: FromStr<Err = E>,
    {
        K::from_str(self.to_string().as_str())
    }
}

impl From<String> for Path {
    fn from(s: String) -> Self {
        let mut identifiers = vec![];
        let parts = s.split('/'); // split will never return an empty iterator
        for part in parts {
            identifiers.push(Identifier::from(part.to_owned()));
        }
        Self(identifiers)
    }
}

impl TryFrom<&[u8]> for Path {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let s = from_utf8(value).map_err(|e| Error::MalformedPathString { error: e })?;
        Ok(s.to_owned().into())
    }
}

impl From<Identifier> for Path {
    fn from(id: Identifier) -> Self {
        Self(vec![id])
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|iden| iden.as_str().to_owned())
                .collect::<Vec<String>>()
                .join("/")
        )
    }
}

impl AsBytes for Path {
    fn as_bytes(&self) -> ByteSlice<'_> {
        ByteSlice::Vector(self.to_string().into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_test() {
        let bytes: &[u8] = b"hello/world";
        assert!(Path::try_from(bytes).is_ok());
    }

    #[test]
    fn sad_test() {
        let bytes: &[u8] = b"hello/\xf0\x28\x8c\xbc";
        assert!(Path::try_from(bytes).is_err());
    }
}
