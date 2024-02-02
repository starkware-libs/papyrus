pub mod behaviour;

use prost_types::Timestamp;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, GasPrice};
use starknet_api::core::GlobalRoot;
use starknet_api::crypto::Signature;
use starknet_types_core::felt::Felt;

use crate::messages::{protobuf, ProtobufConversionError};
use crate::streamed_data::{self, SessionId};
use crate::{BlockHashOrNumber, BlockQuery, Direction};

#[derive(thiserror::Error, Debug)]
pub enum SessionError {
    #[error(transparent)]
    StreamedData(#[from] streamed_data::behaviour::SessionError),
    #[error("Incompatible data error")]
    IncompatibleDataError,
    #[error(transparent)]
    ProtobufConversionError(#[from] ProtobufConversionError),
    #[error("Pairing of header and signature error")]
    PairingError,
    #[error("Session closed unexpectedly")]
    SessionClosedUnexpectedly,
    #[error("Waiting to complete pairing of header and signature")]
    WaitingToCompletePairing,
    // TODO: cast the i32 to the enum value of the error it represents.
    #[error("Received fin")]
    ReceivedFin(i32),
    #[error("Incorrect session id")]
    IncorrectSessionId,
    #[error("Received a message after Fin")]
    ReceivedMessageAfterFin,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum Event {
    NewInboundQuery {
        query: BlockQuery,
        inbound_session_id: streamed_data::InboundSessionId,
    },
    ReceivedData {
        data: Vec<BlockHeaderData>,
        outbound_session_id: streamed_data::OutboundSessionId,
    },
    SessionFailed {
        session_id: SessionId,
        session_error: SessionError,
    },
    QueryConversionError(ProtobufConversionError),
    SessionCompletedSuccessfully {
        session_id: SessionId,
    },
}

impl TryFrom<protobuf::BlockHeadersRequest> for BlockQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersRequest) -> Result<Self, Self::Error> {
        if let Some(value) = value.iteration {
            if let Some(start) = value.start {
                let start_block = match start {
                    protobuf::iteration::Start::BlockNumber(block_number) => {
                        BlockHashOrNumber::Number(BlockNumber(block_number))
                    }
                    protobuf::iteration::Start::Header(protobuf::Hash { elements: bytes }) => {
                        let bytes: [u8; 32] = bytes
                            .try_into()
                            .map_err(|_| ProtobufConversionError::BytesDataLengthMismatch)?;
                        let block_hash = BlockHash(Felt::from_bytes_be(&bytes));
                        if bytes != block_hash.0.to_bytes_be() {
                            // OutOfRange is the only StarknetApiError that Felt::new will
                            // practically return
                            // TODO(shahak): Enforce Felt::new to return only OutOfRange by
                            // defining a more limited StarknetApiError.
                            return Err(ProtobufConversionError::OutOfRangeValue);
                        }
                        BlockHashOrNumber::Hash(block_hash)
                    }
                };
                let direction = match value.direction {
                    0 => Direction::Forward,
                    1 => Direction::Backward,
                    _ => return Err(ProtobufConversionError::OutOfRangeValue),
                };
                let limit = value.limit;
                let step = value.step;
                Ok(Self { start_block, direction, limit, step })
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
                    start: match value.start_block {
                        BlockHashOrNumber::Number(BlockNumber(num)) => {
                            Some(protobuf::iteration::Start::BlockNumber(num))
                        }
                        BlockHashOrNumber::Hash(BlockHash(stark_hash)) => {
                            Some(protobuf::iteration::Start::Header(protobuf::Hash {
                                elements: stark_hash.to_bytes_be().to_vec(),
                            }))
                        }
                    },
                }
            }),
        }
    }
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
        let parent_header = value
            .parent_header
            .and_then(|parent_header| parent_header.try_into().map(BlockHash).ok())
            .ok_or(ProtobufConversionError::MissingField)?;

        let sequencer_address = value
            .sequencer_address
            .and_then(|sequencer_address| sequencer_address.try_into().ok())
            .ok_or(ProtobufConversionError::MissingField)?;

        let timestamp = value
            .time
            .and_then(|timestamp| {
                timestamp.seconds.try_into().map(starknet_api::block::BlockTimestamp).ok()
            })
            .ok_or(ProtobufConversionError::MissingField)?;

        let state_root = value
            .state
            .and_then(|state_root| {
                state_root.root.and_then(|root_hash| root_hash.try_into().map(GlobalRoot).ok())
            })
            .ok_or(ProtobufConversionError::MissingField)?;

        Ok(BlockHeader {
            parent_hash: parent_header,
            block_number: BlockNumber(value.number),
            sequencer: sequencer_address,
            timestamp,
            state_root,
            // TODO: add missing fields.
            block_hash: BlockHash::default(),
            eth_l1_gas_price: GasPrice::default(),
            strk_l1_gas_price: GasPrice::default(),
        })
    }
}

#[derive(Debug)]
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

impl From<BlockHeader> for protobuf::BlockHeader {
    fn from(value: BlockHeader) -> Self {
        Self {
            parent_header: Some(protobuf::Hash {
                elements: value.parent_hash.0.to_bytes_be().to_vec(),
            }),
            number: value.block_number.0,
            sequencer_address: Some(protobuf::Address {
                elements: value.sequencer.0.to_bytes_be().to_vec(),
            }),
            state: Some(protobuf::Patricia {
                root: Some(protobuf::Hash { elements: value.state_root.0.to_bytes_be().to_vec() }),
                height: 0,
            }),
            // TODO: fix timestamp conversion and
            time: Some(Timestamp { seconds: value.timestamp.0.try_into().unwrap_or(0), nanos: 0 }),
            // TODO: add missing fields.
            state_diffs: None,
            proof_fact: None,
            transactions: None,
            events: None,
            receipts: None,
        }
    }
}

impl From<starknet_api::block::BlockSignature> for protobuf::ConsensusSignature {
    fn from(value: starknet_api::block::BlockSignature) -> Self {
        Self { r: Some(value.0.r.into()), s: Some(value.0.s.into()) }
    }
}
