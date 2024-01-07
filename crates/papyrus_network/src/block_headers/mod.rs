pub mod behaviour;
#[cfg(test)]
mod behaviour_test;
#[cfg(test)]
mod flow_test;

use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::crypto::Signature;

use crate::messages::{protobuf, ProtobufConversionError};
use crate::streamed_data::{self, SessionId};
use crate::{BlockQuery, Direction};

#[derive(thiserror::Error, Debug)]
pub(crate) enum SessionError {
    #[error(transparent)]
    StreamedData(#[from] streamed_data::behaviour::SessionError),
    #[error("Incompatible data error")]
    IncompatibleDataError,
    #[error("Pairing of header and signature error")]
    PairingError,
    #[error("Session closed unexpectedly")]
    SessionClosedUnexpectedly,
    #[error("Waiting to complete pairing of header and signature")]
    WaitingToCompletePairing,
    #[error("Received fin")]
    ReceivedFin,
    #[error("Incorrect session id")]
    IncorrectSessionId,
}

#[cfg_attr(test, derive(Debug))]
#[allow(dead_code)]
pub(crate) enum Event {
    NewInboundQuery { query: BlockQuery, inbound_session_id: streamed_data::InboundSessionId },
    ReceivedData { data: BlockHeaderData, outbound_session_id: streamed_data::OutboundSessionId },
    SessionFailed { session_id: SessionId, session_error: SessionError },
    ProtobufConversionError(ProtobufConversionError),
    SessionCompletedSuccessfully { session_id: SessionId },
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
                            _ => return Err(ProtobufConversionError::OutOfRangeValue),
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

impl From<BlockQuery> for protobuf::BlockHeadersRequest {
    fn from(value: BlockQuery) -> Self {
        protobuf::BlockHeadersRequest {
            iteration: Some({
                protobuf::Iteration {
                    direction: match value.direction {
                        Direction::Forward => 0,
                        Direction::Backward => 1,
                    },
                    limit: value.limit,
                    step: value.step,
                    start: Some(protobuf::iteration::Start::BlockNumber(value.start_block.0)),
                }
            }),
        }
    }
}

// TODO(nevo): decide if we need this struct or we can covert the protobuf directly to starknet api
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

impl TryFrom<protobuf::ConsensusSignature> for Signature {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ConsensusSignature) -> Result<Self, Self::Error> {
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

impl TryFrom<protobuf::Signatures> for Vec<Signature> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Signatures) -> Result<Self, Self::Error> {
        let mut signatures = Vec::with_capacity(value.signatures.len());
        for signature in value.signatures {
            signatures.push(signature.try_into()?);
        }
        Ok(signatures)
    }
}
