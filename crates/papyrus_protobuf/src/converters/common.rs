use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::data_availability::{DataAvailabilityMode, L1DataAvailabilityMode};

use super::ProtobufConversionError;
use crate::protobuf;
use crate::sync::{BlockHashOrNumber, Direction, Query};

#[cfg(test)]
#[allow(dead_code)]
pub const PATRICIA_HEIGHT: u32 = 251;

impl TryFrom<protobuf::Felt252> for starknet_types_core::felt::Felt {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Felt252) -> Result<Self, Self::Error> {
        let mut felt = [0; 32];
        felt.copy_from_slice(&value.elements);
        // TODO: use from_bytes_checked once it's available.
        Ok(Self::from_bytes_be(&felt))
        // if let Ok(stark_felt) = Self::from_bytes_be(&felt) {
        //     Ok(stark_felt)
        // } else {
        //     Err(ProtobufConversionError::OutOfRangeValue {
        //         type_description: "Felt252",
        //         value_as_str: format!("{felt:?}"),
        //     })
        // }
    }
}

impl From<starknet_types_core::felt::Felt> for protobuf::Felt252 {
    fn from(value: starknet_types_core::felt::Felt) -> Self {
        Self { elements: value.to_bytes_be().to_vec() }
    }
}

impl From<starknet_api::block::BlockHash> for protobuf::Hash {
    fn from(value: starknet_api::block::BlockHash) -> Self {
        Self { elements: value.0.to_bytes_be().to_vec() }
    }
}

impl From<starknet_api::hash::StarkHash> for protobuf::Hash {
    fn from(value: starknet_api::hash::StarkHash) -> Self {
        Self { elements: value.to_bytes_be().to_vec() }
    }
}

impl From<starknet_api::core::ContractAddress> for protobuf::Address {
    fn from(value: starknet_api::core::ContractAddress) -> Self {
        Self { elements: value.0.key().to_bytes_be().to_vec() }
    }
}

impl From<u128> for protobuf::Uint128 {
    fn from(value: u128) -> Self {
        Self { high: (value >> 64) as u64, low: value as u64 }
    }
}

impl From<protobuf::Uint128> for u128 {
    fn from(value: protobuf::Uint128) -> Self {
        u128::from(value.low) + (u128::from(value.high) << 64)
    }
}

impl TryFrom<protobuf::Hash> for starknet_api::hash::StarkHash {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Hash) -> Result<Self, Self::Error> {
        let mut felt = [0; 32];
        if value.elements.len() != 32 {
            return Err(ProtobufConversionError::BytesDataLengthMismatch {
                type_description: "Hash",
                num_expected: 32,
                value: value.elements,
            });
        }
        felt.copy_from_slice(&value.elements);
        // TODO: use from_bytes_checked once it's available.
        Ok(Self::from_bytes_be(&felt))
        // if let Ok(stark_hash) = Self::new(felt) {
        //     Ok(stark_hash)
        // } else {
        //     Err(ProtobufConversionError::OutOfRangeValue {
        //         type_description: "Hash",
        //         value_as_str: format!("{felt:?}"),
        //     })
        // }
    }
}

impl TryFrom<protobuf::Address> for starknet_api::core::ContractAddress {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Address) -> Result<Self, Self::Error> {
        let mut felt = [0; 32];
        if value.elements.len() != 32 {
            return Err(ProtobufConversionError::BytesDataLengthMismatch {
                type_description: "Address",
                num_expected: 32,
                value: value.elements,
            });
        }
        felt.copy_from_slice(&value.elements);
        // TODO: use from_bytes_checked once it's available.
        let hash = starknet_types_core::felt::Felt::from_bytes_be(&felt);
        // if let Ok(hash) = starknet_api::hash::StarkHash::new(felt) {
        if let Ok(stark_felt) = starknet_api::core::PatriciaKey::try_from(hash) {
            Ok(starknet_api::core::ContractAddress(stark_felt))
        } else {
            Err(ProtobufConversionError::OutOfRangeValue {
                type_description: "Address",
                value_as_str: format!("{felt:?}"),
            })
        }
        // } else {
        //     Err(ProtobufConversionError::OutOfRangeValue {
        //         type_description: "Address",
        //         value_as_str: format!("{felt:?}"),
        //     })
        // }
    }
}

pub(super) fn enum_int_to_l1_data_availability_mode(
    value: i32,
) -> Result<L1DataAvailabilityMode, ProtobufConversionError> {
    match value {
        0 => Ok(L1DataAvailabilityMode::Calldata),
        1 => Ok(L1DataAvailabilityMode::Blob),
        _ => Err(ProtobufConversionError::OutOfRangeValue {
            type_description: "DataAvailabilityMode",
            value_as_str: format!("{value}"),
        }),
    }
}

pub(super) fn l1_data_availability_mode_to_enum_int(value: L1DataAvailabilityMode) -> i32 {
    match value {
        L1DataAvailabilityMode::Calldata => 0,
        L1DataAvailabilityMode::Blob => 1,
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) trait TestInstance {
    fn test_instance() -> Self;
}

#[cfg(test)]
impl TestInstance for protobuf::Hash {
    fn test_instance() -> Self {
        Self { elements: [0].repeat(32).to_vec() }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::Address {
    fn test_instance() -> Self {
        Self { elements: [0].repeat(32).to_vec() }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::Patricia {
    fn test_instance() -> Self {
        Self { n_leaves: 0, root: Some(protobuf::Hash::test_instance()) }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::Uint128 {
    fn test_instance() -> Self {
        Self { low: 1, high: 0 }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::ConsensusSignature {
    fn test_instance() -> Self {
        Self {
            r: Some(protobuf::Felt252 { elements: [1].repeat(32).to_vec() }),
            s: Some(protobuf::Felt252 { elements: [1].repeat(32).to_vec() }),
        }
    }
}

#[allow(dead_code)]
pub(super) fn enum_int_to_volition_domain(
    value: i32,
) -> Result<DataAvailabilityMode, ProtobufConversionError> {
    match value {
        0 => Ok(DataAvailabilityMode::L1),
        1 => Ok(DataAvailabilityMode::L2),
        _ => Err(ProtobufConversionError::OutOfRangeValue {
            type_description: "VolitionDomain",
            value_as_str: format!("{value}"),
        }),
    }
}

// TODO(shahak): Internalize this once network doesn't depend on protobuf.
pub fn volition_domain_to_enum_int(value: DataAvailabilityMode) -> i32 {
    match value {
        DataAvailabilityMode::L1 => 0,
        DataAvailabilityMode::L2 => 1,
    }
}

impl TryFrom<protobuf::Iteration> for Query {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::Iteration) -> Result<Self, Self::Error> {
        let start = value.start.ok_or(ProtobufConversionError::MissingField {
            field_description: "Iteration::start",
        })?;
        let start_block = match start {
            protobuf::iteration::Start::BlockNumber(block_number) => {
                BlockHashOrNumber::Number(BlockNumber(block_number))
            }
            protobuf::iteration::Start::Header(protobuf_hash) => {
                BlockHashOrNumber::Hash(BlockHash(protobuf_hash.try_into()?))
            }
        };
        let direction = match value.direction {
            0 => Direction::Forward,
            1 => Direction::Backward,
            direction => {
                return Err(ProtobufConversionError::OutOfRangeValue {
                    type_description: "Direction",
                    value_as_str: format!("{direction}"),
                });
            }
        };
        let limit = value.limit;
        let step = value.step;
        Ok(Query { start_block, direction, limit, step })
    }
}

impl From<Query> for protobuf::Iteration {
    fn from(value: Query) -> Self {
        let start = match value.start_block {
            BlockHashOrNumber::Number(BlockNumber(number)) => {
                protobuf::iteration::Start::BlockNumber(number)
            }
            BlockHashOrNumber::Hash(block_hash) => {
                protobuf::iteration::Start::Header(block_hash.into())
            }
        };
        Self {
            start: Some(start),
            direction: match value.direction {
                Direction::Forward => 0,
                Direction::Backward => 1,
            },
            limit: value.limit,
            step: value.step,
        }
    }
}

// TODO: Consider add this functionality to the Felt itself.
pub(super) fn try_from_starkfelt_to_u128(
    felt: starknet_types_core::felt::Felt,
) -> Result<u128, &'static str> {
    const COMPLIMENT_OF_U128: usize = 16; // 32 - 16
    let felt_be_bytes = felt.to_bytes_be();
    let (rest, u128_bytes) = felt_be_bytes.split_at(COMPLIMENT_OF_U128);
    if rest != [0u8; COMPLIMENT_OF_U128] {
        return Err("Value out of range");
    }

    let bytes: [u8; 16] = match u128_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return Err("Failed to convert bytes to u128"),
    };

    Ok(u128::from_be_bytes(bytes))
}

// TODO: Consider add this functionality to the Felt itself.
pub(super) fn try_from_starkfelt_to_u32(
    felt: starknet_types_core::felt::Felt,
) -> Result<u32, &'static str> {
    const COMPLIMENT_OF_U32: usize = 28; // 32 - 4
    let felt_be_bytes = felt.to_bytes_be();
    let (rest, u32_bytes) = felt_be_bytes.split_at(COMPLIMENT_OF_U32);
    if rest != [0u8; COMPLIMENT_OF_U32] {
        return Err("Value out of range");
    }

    let bytes: [u8; 4] = match u32_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return Err("Failed to convert bytes to u32"),
    };

    Ok(u32::from_be_bytes(bytes))
}
