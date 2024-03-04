pub mod behaviour;

use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::crypto::Signature;
use starknet_api::hash::StarkHash;

#[cfg(test)]
use crate::messages::TestInstance;
use crate::messages::{l1_data_availability_mode_to_enum_int, protobuf, ProtobufConversionError};
use crate::streamed_data::{self, SessionId};
use crate::{BlockHashOrNumber, Direction, InternalQuery, SignedBlockHeader};

// TODO(shahak) Consider splitting to InboundSessionError and OutboundSessionError.
#[derive(thiserror::Error, Debug)]
pub enum SessionError {
    #[error(transparent)]
    StreamedData(#[from] streamed_data::behaviour::SessionError),
    // This error can only appear in outbound sessions
    #[error(transparent)]
    ProtobufConversionError(#[from] ProtobufConversionError),
    #[error("Session closed unexpectedly")]
    SessionClosedUnexpectedly,
    // This error can only appear in outbound sessions
    #[error("Received a message after Fin")]
    ReceivedMessageAfterFin,
}

#[derive(Debug)]
#[allow(dead_code)]
// TODO(shahak): Internalize this when we have a mixed behaviour.
pub enum Event {
    NewInboundQuery {
        query: InternalQuery,
        inbound_session_id: streamed_data::InboundSessionId,
    },
    ReceivedData {
        signed_header: SignedBlockHeader,
        outbound_session_id: streamed_data::OutboundSessionId,
    },
    SessionFailed {
        session_id: SessionId,
        session_error: SessionError,
    },
    QueryConversionError(ProtobufConversionError),
    SessionFinishedSuccessfully {
        session_id: SessionId,
    },
}

impl TryFrom<protobuf::BlockHeadersRequest> for InternalQuery {
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
                        let block_hash = BlockHash(StarkHash::new(bytes).map_err(|_| {
                            // OutOfRange is the only StarknetApiError that StarkHash::new will
                            // practically return
                            // TODO(shahak): Enforce StarkHash::new to return only OutOfRange by
                            // defining a more limited StarknetApiError.
                            ProtobufConversionError::OutOfRangeValue
                        })?);
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

impl From<InternalQuery> for protobuf::BlockHeadersRequest {
    fn from(value: InternalQuery) -> Self {
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
                                elements: stark_hash.bytes().to_vec(),
                            }))
                        }
                    },
                }
            }),
        }
    }
}

impl From<(BlockHeader, BlockSignature)> for protobuf::SignedBlockHeader {
    fn from((header, signature): (BlockHeader, BlockSignature)) -> Self {
        Self {
            block_hash: Some(header.block_hash.into()),
            parent_hash: Some(header.parent_hash.into()),
            number: header.block_number.0,
            time: header.timestamp.0,
            sequencer_address: Some(header.sequencer.0.into()),
            // TODO(shahak): fill this.
            state_diff_commitment: None,
            state: Some(protobuf::Patricia {
                // TODO(shahak): fill this.
                height: 0,
                root: Some(header.state_root.0.into()),
            }),
            transactions: Some(protobuf::Merkle {
                n_leaves: header
                    .n_transactions
                    .unwrap_or_default()
                    .try_into()
                    .expect("Converting usize to u64 failed"),
                root: Some(header.transaction_commitment.unwrap_or_default().0.into()),
            }),
            events: Some(protobuf::Merkle {
                n_leaves: header
                    .n_events
                    .unwrap_or_default()
                    .try_into()
                    .expect("Converting usize to u64 failed"),
                root: Some(header.event_commitment.unwrap_or_default().0.into()),
            }),
            // TODO(shahak): fill this.
            receipts: None,
            protocol_version: header.starknet_version.0,
            gas_price_wei: Some(header.l1_gas_price.price_in_wei.0.into()),
            gas_price_fri: Some(header.l1_gas_price.price_in_fri.0.into()),
            data_gas_price_wei: Some(header.l1_data_gas_price.price_in_wei.0.into()),
            data_gas_price_fri: Some(header.l1_data_gas_price.price_in_fri.0.into()),
            l1_data_availability_mode: l1_data_availability_mode_to_enum_int(header.l1_da_mode),
            // TODO(shahak): fill this.
            num_storage_diffs: 0,
            // TODO(shahak): fill this.
            num_nonce_updates: 0,
            // TODO(shahak): fill this.
            num_declared_classes: 0,
            // TODO(shahak): fill this.
            num_deployed_contracts: 0,
            // TODO(shahak): fill this.
            signatures: vec![signature.into()],
        }
    }
}

impl TryFrom<protobuf::ConsensusSignature> for starknet_api::block::BlockSignature {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ConsensusSignature) -> Result<Self, Self::Error> {
        Ok(Self(Signature {
            r: value.r.ok_or(ProtobufConversionError::MissingField)?.try_into()?,
            s: value.s.ok_or(ProtobufConversionError::MissingField)?.try_into()?,
        }))
    }
}

#[cfg(test)]
impl TestInstance for protobuf::SignedBlockHeader {
    fn test_instance() -> Self {
        Self {
            block_hash: Some(protobuf::Hash::test_instance()),
            parent_hash: Some(protobuf::Hash::test_instance()),
            number: 1,
            time: 1,
            sequencer_address: Some(protobuf::Address::test_instance()),
            state_diff_commitment: Some(protobuf::Hash::test_instance()),
            state: Some(protobuf::Patricia::test_instance()),
            transactions: Some(protobuf::Merkle::test_instance()),
            events: Some(protobuf::Merkle::test_instance()),
            receipts: Some(protobuf::Merkle::test_instance()),
            protocol_version: "0.0.0".to_owned(),
            gas_price_fri: Some(protobuf::Uint128::test_instance()),
            gas_price_wei: Some(protobuf::Uint128::test_instance()),
            data_gas_price_fri: Some(protobuf::Uint128::test_instance()),
            data_gas_price_wei: Some(protobuf::Uint128::test_instance()),
            l1_data_availability_mode: 0,
            num_storage_diffs: 0,
            num_nonce_updates: 0,
            num_declared_classes: 0,
            num_deployed_contracts: 0,
            signatures: vec![protobuf::ConsensusSignature::test_instance()],
        }
    }
}
