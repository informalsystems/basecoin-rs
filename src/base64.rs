extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S: Serializer>(v: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    let mut buf = String::new();
    base64::encode_config_buf(v, base64::STANDARD, &mut buf);

    String::serialize(&buf, serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let base64 = String::deserialize(deserializer)?;

    let mut buf = Vec::new();
    base64::decode_config_buf(base64.as_bytes(), base64::STANDARD, &mut buf)
        .map_err(serde::de::Error::custom)?;

    Ok(buf)
}
