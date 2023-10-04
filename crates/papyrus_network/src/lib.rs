/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub(crate) mod block_headers_protocol;
mod db_executor;
pub mod messages;
mod streamed_data_protocol;
#[cfg(test)]
mod test_utils;

use messages::proto::p2p::proto::ConsensusSignature;
use messages::{protobuf, ProtobufConversionError};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkFelt;

#[cfg_attr(test, derive(Debug))]
pub enum Direction {
    Forward,
    Backward,
}

#[cfg_attr(test, derive(Debug))]
pub struct BlockQuery {
    pub start_block: BlockNumber,
    pub direction: Direction,
    pub limit: u64,
    pub step: u64,
}

impl TryFrom<protobuf::BlockHeadersRequest> for BlockQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersRequest) -> Result<Self, Self::Error> {
        if let Some(value) = value.iteration {
            if let Some(start) = value.start {
                match start {
                    protobuf::iteration::Start::BlockNumber(block_number) => {
                        let start_block = BlockNumber(block_number);
                        let direction = match value.direction {
                            0 => Direction::Forward,
                            1 => Direction::Backward,
                            _ => return Err(ProtobufConversionError::UnexpectedEnumValue),
                        };
                        let limit = value.limit;
                        let step = value.step;
                        Ok(Self { start_block, direction, limit, step })
                    }
                    protobuf::iteration::Start::Header(_) => {
                        unimplemented!("BlockHash is not supported yet")
                    }
                }
            } else {
                Err(ProtobufConversionError::MissingField)
            }
        } else {
            Err(ProtobufConversionError::MissingField)
        }
    }
}

#[derive(Debug)]
pub struct Signature {
    pub r: StarkFelt,
    pub s: StarkFelt,
}

// TODO(nevo): decide if we need this struct or we can covert the protobuf directly to starkent api
// BlockHeader.
#[derive(Debug)]
pub struct BlockHeader {
    pub parent_header: BlockHash,
    pub number: BlockNumber,
    // pub time: BlockTimestamp,
    pub sequencer_address: ContractAddress,
    // pub state_diffs: Option<Merkle>,
    // pub state: Option<Patricia>,
    // pub proof_fact: Option<Hash>,
    // pub transactions: Option<Merkle>,
    // pub events: Option<Merkle>,
    // pub receipts: Option<Merkle>,
}

impl TryFrom<ConsensusSignature> for Signature {
    type Error = ProtobufConversionError;
    fn try_from(value: ConsensusSignature) -> Result<Self, Self::Error> {
        let r = match value.r {
            Some(r) => r.try_into(),
            None => return Err(ProtobufConversionError::MissingField),
        }?;
        let s = match value.s {
            Some(s) => s.try_into(),
            None => return Err(ProtobufConversionError::MissingField),
        }?;
        Ok(Self { r, s })
    }
}

impl TryFrom<protobuf::BlockHeader> for BlockHeader {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeader) -> Result<Self, Self::Error> {
        let parent_header = match value.parent_header {
            Some(parent_header) => {
                if let Ok(hash) = parent_header.try_into() {
                    Ok(BlockHash(hash))
                } else {
                    Err(ProtobufConversionError::MissingField)
                }
            }
            None => return Err(ProtobufConversionError::MissingField),
        }?;
        let sequencer_address = match value.sequencer_address {
            Some(sequencer_address) => sequencer_address.try_into(),
            None => return Err(ProtobufConversionError::MissingField),
        }?;
        Ok(BlockHeader { parent_header, number: BlockNumber(value.number), sequencer_address })
    }
}
#[cfg_attr(test, derive(Debug))]
pub struct BlockHeaderData {
    pub block_header: BlockHeader,
    pub signatures: Vec<Signature>,
}
