use serde::{de::Visitor, Deserialize, Serialize};

use crate::starknet;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "HexAsBytes<32>")]
pub struct StarkHash(pub [u8; 32]);

impl From<HexAsBytes<32_usize>> for StarkHash {
    fn from(v: HexAsBytes<32_usize>) -> Self {
        StarkHash(v.0)
    }
}
impl From<StarkHash> for starknet::StarkHash {
    fn from(val: StarkHash) -> Self {
        starknet::StarkHash(val.0)
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHash(pub StarkHash);
impl From<BlockHash> for starknet::BlockHash {
    fn from(val: BlockHash) -> Self {
        starknet::BlockHash(val.0.into())
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ContractAddress(pub StarkHash);
impl From<ContractAddress> for starknet::ContractAddress {
    fn from(val: ContractAddress) -> Self {
        starknet::ContractAddress(val.0.into())
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct GlobalRoot(pub StarkHash);
impl From<GlobalRoot> for starknet::GlobalRoot {
    fn from(val: GlobalRoot) -> Self {
        starknet::GlobalRoot(val.0.into())
    }
}
#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockNumber(pub u64);
impl From<BlockNumber> for starknet::BlockNumber {
    fn from(val: BlockNumber) -> Self {
        starknet::BlockNumber(val.0)
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(from = "HexAsBytes<16>")]
pub struct GasPrice(pub u128);
impl From<HexAsBytes<16_usize>> for GasPrice {
    fn from(v: HexAsBytes<16_usize>) -> Self {
        GasPrice(u128::from_be_bytes(v.0))
    }
}
impl From<GasPrice> for starknet::GasPrice {
    fn from(val: GasPrice) -> Self {
        starknet::GasPrice(val.0)
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockTimestamp(pub u64);
impl From<BlockTimestamp> for starknet::BlockTimestamp {
    fn from(val: BlockTimestamp) -> Self {
        starknet::BlockTimestamp(val.0)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Block {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub gas_price: GasPrice,
    pub parent_block_hash: BlockHash,
    pub sequencer_address: ContractAddress,
    pub state_root: GlobalRoot,
    pub status: BlockStatus,
    pub timestamp: BlockTimestamp,
    // TODO(dan): define corresponding structs and handle properly.
    transaction_receipts: Vec<serde_json::Value>,
    transactions: Vec<serde_json::Value>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    #[serde(rename(deserialize = "ABORTED"))]
    Aborted,
    #[serde(rename(deserialize = "ACCEPTED_ON_L1"))]
    AcceptedOnL1,
    #[serde(rename(deserialize = "ACCEPTED_ON_L2"))]
    AcceptedOnL2,
    #[serde(rename(deserialize = "PENDING"))]
    Pending,
    #[serde(rename(deserialize = "REVERTED"))]
    Reverted,
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
                bytes_from_hex_str::<N>(v)
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
    #[error(
        "Bad input - expected #bytes: {expected_byte_count:?}, string found: {string_found:?}."
    )]
    BadInput {
        expected_byte_count: usize,
        string_found: String,
    },
}

pub fn bytes_from_hex_str<const N: usize>(hex_str: &str) -> Result<[u8; N], DeserializationError> {
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);

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

    hex::decode(&padded_str)?
        .try_into()
        .map_err(|_| DeserializationError::BadInput {
            expected_byte_count: N,
            string_found: padded_str,
        })
}
