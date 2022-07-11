#[cfg(test)]
#[path = "serde_utils_test.rs"]
mod serde_utils_test;

use serde::de::Visitor;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct HexAsBytes<const N: usize, const PREFIXED: bool>(pub [u8; N]);

pub type PrefixedHexAsBytes<const N: usize> = HexAsBytes<N, true>;
pub type NonPrefixedHexAsBytes<const N: usize> = HexAsBytes<N, false>;

impl<'de, const N: usize, const PREFIXED: bool> Deserialize<'de> for HexAsBytes<N, PREFIXED> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HexStringVisitor<const N: usize, const PREFIXED: bool>;

        impl<'de, const N: usize, const PREFIXED: bool> Visitor<'de> for HexStringVisitor<N, PREFIXED> {
            type Value = HexAsBytes<N, PREFIXED>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a hex string, possibly prefixed by '0x'")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                bytes_from_hex_str::<N, PREFIXED>(v)
                    .map_err(serde::de::Error::custom)
                    .map(HexAsBytes)
            }
        }

        deserializer.deserialize_str(HexStringVisitor)
    }
}

impl<const N: usize, const PREFIXED: bool> Serialize for HexAsBytes<N, PREFIXED> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hex_str = hex_str_from_bytes::<N, PREFIXED>(self.0);
        serializer.serialize_str(&hex_str)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DeserializationError {
    #[error(transparent)]
    FromHexError(#[from] hex::FromHexError),
    #[error("Missing prefix 0x in {hex_str}")]
    MissingPrefix { hex_str: String },
    #[error("Bad input - expected #bytes: {expected_byte_count}, string found: {string_found}.")]
    BadInput {
        expected_byte_count: usize,
        string_found: String,
    },
}

pub fn bytes_from_hex_str<const N: usize, const PREFIXED: bool>(
    hex_str: &str,
) -> Result<[u8; N], DeserializationError> {
    let hex_str = if PREFIXED {
        hex_str
            .strip_prefix("0x")
            .ok_or(DeserializationError::MissingPrefix {
                hex_str: hex_str.into(),
            })?
    } else {
        hex_str
    };

    // Make sure string is not too long.
    if hex_str.len() > 2 * N {
        let mut err_str = "0x".to_owned();
        err_str.push_str(hex_str);
        return Err(DeserializationError::BadInput {
            expected_byte_count: N,
            string_found: err_str,
        });
    }

    // Pad if needed.
    let to_add = 2 * N - hex_str.len();
    let padded_str = vec!["0"; to_add].join("") + hex_str;

    Ok(hex::decode(&padded_str)?
        .try_into()
        .expect("Unexpected length of deserialized hex bytes."))
}

pub fn hex_str_from_bytes<const N: usize, const PREFIXED: bool>(bytes: [u8; N]) -> String {
    let hex_str = hex::encode(bytes);
    let mut hex_str = hex_str.trim_start_matches('0');
    hex_str = if hex_str.is_empty() { "0" } else { hex_str };
    if PREFIXED {
        format!("0x{}", hex_str)
    } else {
        hex_str.to_string()
    }
}
