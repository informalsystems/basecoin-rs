//! Common code for encoding-related concerns.

use bytes::BufMut;

/// Encode the given value as a variable-length integer into the given buffer.
pub fn encode_varint<B: BufMut>(val: u64, mut buf: &mut B) {
    prost::encoding::encode_varint(val << 1, &mut buf);
}
