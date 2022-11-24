//! Utilities for serialising/deserialising hexadecimal values.
#[cfg(test)]
#[path = "serde_utils_test.rs"]
mod serde_utils_test;

use serde::de::{Deserialize, Visitor};
use serde::ser::{Serialize, SerializeTuple};

/// A [BytesAsHex](`crate::serde_utils::BytesAsHex`) prefixed with '0x'.
pub type PrefixedBytesAsHex<const N: usize> = BytesAsHex<N, true>;

/// A [BytesAsHex](`crate::serde_utils::BytesAsHex`) non-prefixed.
pub type NonPrefixedBytesAsHex<const N: usize> = BytesAsHex<N, false>;

/// A byte array that serializes as a hex string.
///
/// The `PREFIXED` generic type symbolize whether a string representation of the hex value should be
/// prefixed by `0x` or not.
#[derive(Debug, Eq, PartialEq)]
pub struct BytesAsHex<const N: usize, const PREFIXED: bool>(pub(crate) [u8; N]);

impl<'de, const N: usize, const PREFIXED: bool> Deserialize<'de> for BytesAsHex<N, PREFIXED> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ByteArrayVisitor<const N: usize, const PREFIXED: bool>;
        impl<'de, const N: usize, const PREFIXED: bool> Visitor<'de> for ByteArrayVisitor<N, PREFIXED> {
            type Value = BytesAsHex<N, PREFIXED>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a byte array")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut res = [0u8; N];
                let mut i = 0;
                while let Some(value) = seq.next_element()? {
                    res[i] = value;
                    i += 1;
                }
                Ok(BytesAsHex(res))
            }
        }

        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            bytes_from_hex_str::<N, PREFIXED>(s.as_str())
                .map_err(serde::de::Error::custom)
                .map(BytesAsHex)
        } else {
            deserializer.deserialize_tuple(N, ByteArrayVisitor)
        }
    }
}

impl<const N: usize, const PREFIXED: bool> Serialize for BytesAsHex<N, PREFIXED> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            let hex_str = hex_str_from_bytes::<N, PREFIXED>(self.0);
            serializer.serialize_str(&hex_str)
        } else {
            let mut seq = serializer.serialize_tuple(N)?;
            for element in &self.0[..] {
                seq.serialize_element(element)?;
            }
            seq.end()
        }
    }
}

/// The error type returned by the inner deserialization.
#[derive(thiserror::Error, Clone, Debug)]
pub enum InnerDeserializationError {
    /// Error parsing the hex string.
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
    /// Missing 0x prefix in the hex string.
    #[error("Missing prefix 0x in {hex_str}")]
    MissingPrefix { hex_str: String },
    /// Unexpected input byte count.
    #[error("Bad input - expected #bytes: {expected_byte_count}, string found: {string_found}.")]
    BadInput { expected_byte_count: usize, string_found: String },
}

/// Deserializes a Hex decoded as string to a byte array.
pub fn bytes_from_hex_str<const N: usize, const PREFIXED: bool>(
    hex_str: &str,
) -> Result<[u8; N], InnerDeserializationError> {
    let hex_str = if PREFIXED {
        hex_str
            .strip_prefix("0x")
            .ok_or(InnerDeserializationError::MissingPrefix { hex_str: hex_str.into() })?
    } else {
        hex_str
    };

    // Make sure string is not too long.
    if hex_str.len() > 2 * N {
        let mut err_str = "0x".to_owned();
        err_str.push_str(hex_str);
        return Err(InnerDeserializationError::BadInput {
            expected_byte_count: N,
            string_found: err_str,
        });
    }

    // Pad if needed.
    let to_add = 2 * N - hex_str.len();
    let padded_str = vec!["0"; to_add].join("") + hex_str;

    Ok(hex::decode(&padded_str)?.try_into().expect("Unexpected length of deserialized hex bytes."))
}

/// Encodes a byte array to a string.
pub fn hex_str_from_bytes<const N: usize, const PREFIXED: bool>(bytes: [u8; N]) -> String {
    let hex_str = hex::encode(bytes);
    let mut hex_str = hex_str.trim_start_matches('0');
    hex_str = if hex_str.is_empty() { "0" } else { hex_str };
    if PREFIXED { format!("0x{}", hex_str) } else { hex_str.to_string() }
}
