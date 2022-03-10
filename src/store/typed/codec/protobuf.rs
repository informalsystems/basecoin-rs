use super::Codec;

use std::convert::TryInto;
use std::marker::PhantomData;

/// A Protobuf codec that uses `prost` to encode/decode
#[derive(Clone)]
pub(crate) struct ProtobufCodec<T, R> {
    domain_type: PhantomData<T>,
    raw_type: PhantomData<R>,
}

impl<'a, T: Into<R> + Clone, R: TryInto<T> + Default + prost::Message> Codec<'a>
    for ProtobufCodec<T, R>
{
    type Type = T;
    type Encoded = Vec<u8>;

    fn encode(d: &'a Self::Type) -> Option<Self::Encoded> {
        let r = d.clone().into();
        Some(r.encode_to_vec())
    }

    fn decode(bytes: &'a [u8]) -> Option<Self::Type> {
        let r = R::decode(bytes).ok()?;
        r.try_into().ok()
    }
}
