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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub struct GasPrice(pub u128);
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

impl<'de> Deserialize<'de> for GasPrice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct GasPriceVisitor;

        impl<'de> Visitor<'de> for GasPriceVisitor {
            type Value = GasPrice;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter
                    .write_str("a '0x' prefixed string representation of an up to 32 nibbles hex")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let v = v.strip_prefix("0x").unwrap_or(v);
                u128::from_str_radix(v, 16)
                    .map_err(serde::de::Error::custom)
                    .map(GasPrice)
            }
        }

        deserializer.deserialize_str(GasPriceVisitor)
    }
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
