use serde::{de::Visitor, Deserialize, Serialize};

use super::hash::StarkHash;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ContractAddress(pub StarkHash);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHash(pub StarkHash);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct GlobalRoot(pub StarkHash);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockNumber(pub u64);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(from = "HexAsBytes<16>")]
pub struct GasPrice(pub u128);
impl From<HexAsBytes<16_usize>> for GasPrice {
    fn from(v: HexAsBytes<16_usize>) -> Self {
        GasPrice(u128::from_be_bytes(v.0))
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockTimestamp(pub u64);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ListCommitment {
    pub length: u64,
    pub commitment: StarkHash,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionsCommitment(ListCommitment);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventsCommitment(ListCommitment);

/// Block / transaction status.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Status {
    #[serde(rename(deserialize = "NOT_RECEIVED"))]
    NotReceived,
    #[serde(rename(deserialize = "RECEIVED"))]
    Received,
    #[serde(rename(deserialize = "PENDING"))]
    Pending,
    #[serde(rename(deserialize = "REJECTED"))]
    Rejected,
    #[serde(rename(deserialize = "ACCEPTED_ON_L1"))]
    AcceptedOnL1,
    #[serde(rename(deserialize = "ACCEPTED_ON_L2"))]
    AcceptedOnL2,
    #[serde(rename(deserialize = "REVERTED"))]
    Reverted,
    #[serde(rename(deserialize = "ABORTED"))]
    Aborted,
}
#[derive(Debug)]
pub struct HexAsBytes<const N: usize>(pub [u8; N]);

impl<'de, const N: usize> Deserialize<'de> for HexAsBytes<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HexStringVisitor<const N: usize>;

        impl<'de, const N: usize> Visitor<'de> for HexStringVisitor<N> {
            type Value = HexAsBytes<N>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a hex string, possibly prefixed by '0x'")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let v = v.strip_prefix("0x").unwrap_or(v);

                bytes_from_hex_str::<N>(v)
                    .map_err(serde::de::Error::custom)
                    .map(HexAsBytes)
            }
        }

        deserializer.deserialize_str(HexStringVisitor)
    }
}

#[derive(Debug, PartialEq)]
pub enum HexParseError {
    InvalidNibble(u8),
    InvalidLength(usize),
}
impl std::error::Error for HexParseError {}

impl std::fmt::Display for HexParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidNibble(n) => f.write_fmt(format_args!("Invalid nibble found: 0x{:x}", *n)),
            Self::InvalidLength(n) => {
                f.write_fmt(format_args!("More than 64 digits found: {}", *n))
            }
        }
    }
}

fn bytes_from_hex_str<const N: usize>(hex_str: &str) -> Result<[u8; N], HexParseError> {
    fn parse_hex_digit(digit: u8) -> Result<u8, HexParseError> {
        match digit {
            b'0'..=b'9' => Ok(digit - b'0'),
            b'A'..=b'F' => Ok(digit - b'A' + 10),
            b'a'..=b'f' => Ok(digit - b'a' + 10),
            other => Err(HexParseError::InvalidNibble(other)),
        }
    }

    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    if hex_str.len() > N * 2 {
        return Err(HexParseError::InvalidLength(hex_str.len()));
    }

    let mut buf = [0u8; N];

    // We want the result in big-endian so reverse iterate over each pair of nibbles.
    let chunks = hex_str.as_bytes().rchunks_exact(2);

    // Handle a possible odd nibble remaining nibble.
    let odd_nibble = chunks.remainder();
    if !odd_nibble.is_empty() {
        let full_bytes = hex_str.len() / 2;
        buf[N - 1 - full_bytes] = parse_hex_digit(odd_nibble[0])?;
    }

    for (i, c) in chunks.enumerate() {
        // Indexing c[0] and c[1] are safe since chunk-size is 2.
        buf[N - 1 - i] = parse_hex_digit(c[0])? << 4 | parse_hex_digit(c[1])?;
    }

    Ok(buf)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub number: BlockNumber,
    pub gas_price: GasPrice,
    pub state_root: GlobalRoot,
    pub sequencer: ContractAddress,
    pub timestamp: BlockTimestamp,
    // TODO(dan): uncomment and handle.
    // pub transactions_commitment: TransactionsCommitment,
    // pub events_commitment: EventsCommitment,
}

pub struct BlockBody {}
