pub(crate) mod json;
pub(crate) mod protobuf;

/// A trait that defines how types are decoded/encoded.
pub(crate) trait Codec<'a> {
    type Type;
    type Encoded: AsRef<[u8]>;

    fn encode(d: &'a Self::Type) -> Option<Self::Encoded>;

    fn decode(bytes: &'a [u8]) -> Option<Self::Type>;
}
