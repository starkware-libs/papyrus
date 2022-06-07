use serde::de::Visitor;
use serde::Deserialize;

#[derive(Debug)]
pub struct HexAsBytes<const N: usize, const PREFIXED: bool>(pub [u8; N]);

impl<'de, const N: usize, const PREFIXED: bool> Deserialize<'de> for HexAsBytes<N, PREFIXED> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HexStringVisitor<const N: usize, const PREFIXED: bool>;

        impl<'de, const N: usize, const PREFIXED: bool> Visitor<'de> for HexStringVisitor<N, PREFIXED> {
            type Value = HexAsBytes<N, PREFIXED>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
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
