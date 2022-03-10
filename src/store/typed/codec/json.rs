use super::Codec;

use std::marker::PhantomData;

use serde::{de::DeserializeOwned, Serialize};

/// A JSON codec that uses `serde_json` to encode/decode as a JSON string
#[derive(Clone)]
pub(crate) struct JsonCodec<T>(PhantomData<T>);

impl<'a, T: Serialize + DeserializeOwned> Codec<'a> for JsonCodec<T> {
    type Type = T;
    type Encoded = String;

    fn encode(d: &'a Self::Type) -> Option<Self::Encoded> {
        serde_json::to_string(d).ok()
    }

    fn decode(bytes: &'a [u8]) -> Option<Self::Type> {
        let json_string = String::from_utf8(bytes.to_vec()).ok()?;
        serde_json::from_str(&json_string).ok()
    }
}
