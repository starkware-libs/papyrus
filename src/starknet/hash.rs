use serde::{de::Visitor, Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub struct StarkHash(pub [u64; 4]);

#[derive(Debug)]
pub enum ParseError {
    InvalidLength(usize),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLength(n) => {
                f.write_fmt(format_args!("More than 64 nibbles found: {}", *n))
            }
        }
    }
}

impl StarkHash {
    pub fn from_hex_str(hex_str: &str) -> Result<Self, ParseError> {
        let _pure_hex_representation = hex_str.strip_prefix("0x").unwrap_or(hex_str);
        if _pure_hex_representation.len() > 64 {
            return Err(ParseError::InvalidLength(_pure_hex_representation.len()));
        }
        // TODO(dan): convert properly.
        Ok(StarkHash([0, 1, 2, 3]))
    }
}

impl<'de> Deserialize<'de> for StarkHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StarkHashVisitor;

        impl<'de> Visitor<'de> for StarkHashVisitor {
            type Value = StarkHash;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter
                    .write_str("a '0x' prefixed string representation of an up to 64 nibbles hex")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                StarkHash::from_hex_str(v).map_err(|e| serde::de::Error::custom(e))
            }
        }

        deserializer.deserialize_str(StarkHashVisitor)
    }
}
