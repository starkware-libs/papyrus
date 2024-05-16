use starknet_api::block::{
    BlockHash,
    BlockHeader,
    BlockNumber,
    BlockSignature,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    SequencerContractAddress,
    TransactionCommitment,
};
use starknet_api::crypto::Signature;

use super::common::{enum_int_to_l1_data_availability_mode, l1_data_availability_mode_to_enum_int};
use super::{ProtobufConversionError, ProtobufResponseToDataError};
use crate::db_executor::Data;
use crate::protobuf_messages::protobuf::{self};
use crate::{DataType, InternalQuery, Query, SignedBlockHeader};

impl TryFrom<protobuf::BlockHeadersResponse> for Option<SignedBlockHeader> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersResponse) -> Result<Self, Self::Error> {
        match value.header_message {
            Some(protobuf::block_headers_response::HeaderMessage::Header(header)) => {
                Ok(Some(header.try_into()?))
            }
            Some(protobuf::block_headers_response::HeaderMessage::Fin(_)) => Ok(None),
            None => Err(ProtobufConversionError::MissingField {
                field_description: "BlockHeadersResponse::header_message",
            }),
        }
    }
}

impl TryFrom<protobuf::SignedBlockHeader> for SignedBlockHeader {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::SignedBlockHeader) -> Result<Self, Self::Error> {
        let block_hash = value
            .block_hash
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "SignedBlockHeader::block_hash",
            })?
            .try_into()
            .map(BlockHash)?;

        let parent_hash = value
            .parent_hash
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "SignedBlockHeader::parent_hash",
            })?
            .try_into()
            .map(BlockHash)?;

        let timestamp = starknet_api::block::BlockTimestamp(value.time);

        let sequencer = value
            .sequencer_address
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "SignedBlockHeader::sequencer_address",
            })?
            .try_into()
            .map(SequencerContractAddress)?;

        let state_root = value
            .state_root
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "SignedBlockHeader::state_root",
            })?
            .try_into()
            .map(GlobalRoot)?;

        let n_transactions = value.transactions.as_ref().map(|transactions| {
            transactions.n_leaves.try_into().expect("Failed converting u64 to usize")
        });

        let transaction_commitment = value
            .transactions
            .map(|transactions| {
                Ok(TransactionCommitment(
                    transactions
                        .root
                        .ok_or(ProtobufConversionError::MissingField {
                            field_description: "Merkle::root",
                        })?
                        .try_into()?,
                ))
            })
            .transpose()?;

        let n_events = value
            .events
            .as_ref()
            .map(|events| events.n_leaves.try_into().expect("Failed converting u64 to usize"));

        let event_commitment = value
            .events
            .map(|events| {
                Ok(EventCommitment(
                    events
                        .root
                        .ok_or(ProtobufConversionError::MissingField {
                            field_description: "Merkle::root",
                        })?
                        .try_into()?,
                ))
            })
            .transpose()?;

        let state_diff_length = value.state_diff_commitment.as_ref().map(|state_diff_commitment| {
            state_diff_commitment
                .state_diff_length
                .try_into()
                .expect("Failed converting u64 to usize")
        });

        let l1_da_mode = enum_int_to_l1_data_availability_mode(value.l1_data_availability_mode)?;

        let starknet_version = StarknetVersion(value.protocol_version);

        let l1_gas_price = GasPricePerToken {
            price_in_fri: GasPrice(
                value
                    .gas_price_fri
                    .ok_or(ProtobufConversionError::MissingField {
                        field_description: "SignedBlockHeader::gas_price_fri",
                    })?
                    .into(),
            ),
            price_in_wei: GasPrice(
                value
                    .gas_price_wei
                    .ok_or(ProtobufConversionError::MissingField {
                        field_description: "SignedBlockHeader::gas_price_wei",
                    })?
                    .into(),
            ),
        };

        let l1_data_gas_price = GasPricePerToken {
            price_in_fri: GasPrice(
                value
                    .data_gas_price_fri
                    .ok_or(ProtobufConversionError::MissingField {
                        field_description: "SignedBlockHeader::data_gas_price_fri",
                    })?
                    .into(),
            ),
            price_in_wei: GasPrice(
                value
                    .data_gas_price_wei
                    .ok_or(ProtobufConversionError::MissingField {
                        field_description: "SignedBlockHeader::data_gas_price_wei",
                    })?
                    .into(),
            ),
        };

        Ok(SignedBlockHeader {
            block_header: BlockHeader {
                block_hash,
                parent_hash,
                block_number: BlockNumber(value.number),
                l1_gas_price,
                l1_data_gas_price,
                state_root,
                sequencer,
                timestamp,
                l1_da_mode,
                // TODO(shahak): fill this.
                state_diff_commitment: None,
                state_diff_length,
                transaction_commitment,
                event_commitment,
                // TODO(shahak): fill this.
                n_transactions,
                n_events,
                receipt_commitment: None,
                starknet_version,
            },
            // collect will convert from Vec<Result> to Result<Vec>.
            signatures: value
                .signatures
                .into_iter()
                .map(starknet_api::block::BlockSignature::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl From<(BlockHeader, Vec<BlockSignature>)> for protobuf::SignedBlockHeader {
    fn from((header, signatures): (BlockHeader, Vec<BlockSignature>)) -> Self {
        Self {
            block_hash: Some(header.block_hash.into()),
            parent_hash: Some(header.parent_hash.into()),
            number: header.block_number.0,
            time: header.timestamp.0,
            sequencer_address: Some(header.sequencer.0.into()),
            state_diff_commitment: Some(protobuf::StateDiffCommitment {
                state_diff_length: header
                    .state_diff_length
                    .unwrap_or(0)
                    .try_into()
                    .expect("Converting usize to u64 failed"),
                // TODO: fill this.
                root: None,
            }),
            state_root: Some(header.state_root.0.into()),
            // This will be Some only if both n_transactions and transaction_commitment are Some.
            transactions: header.n_transactions.and_then(|n_transactions| {
                header.transaction_commitment.map(|transaction_commitment| protobuf::Patricia {
                    n_leaves: n_transactions.try_into().expect("Converting usize to u64 failed"),
                    root: Some(transaction_commitment.0.into()),
                })
            }),
            // This will be Some only if both n_events and event_commitment are Some.
            events: header.n_events.and_then(|n_events| {
                header.event_commitment.map(|event_commitment| protobuf::Patricia {
                    n_leaves: n_events.try_into().expect("Converting usize to u64 failed"),
                    root: Some(event_commitment.0.into()),
                })
            }),
            // TODO(shahak): fill this.
            receipts: None,
            protocol_version: header.starknet_version.0,
            gas_price_wei: Some(header.l1_gas_price.price_in_wei.0.into()),
            gas_price_fri: Some(header.l1_gas_price.price_in_fri.0.into()),
            data_gas_price_wei: Some(header.l1_data_gas_price.price_in_wei.0.into()),
            data_gas_price_fri: Some(header.l1_data_gas_price.price_in_fri.0.into()),
            l1_data_availability_mode: l1_data_availability_mode_to_enum_int(header.l1_da_mode),
            signatures: signatures.iter().map(|signature| (*signature).into()).collect(),
        }
    }
}

impl TryFrom<protobuf::ConsensusSignature> for starknet_api::block::BlockSignature {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ConsensusSignature) -> Result<Self, Self::Error> {
        Ok(Self(Signature {
            r: value
                .r
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "SignedBlockHeader::r",
                })?
                .try_into()?,
            s: value
                .s
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "SignedBlockHeader::s",
                })?
                .try_into()?,
        }))
    }
}

impl From<starknet_api::block::BlockSignature> for protobuf::ConsensusSignature {
    fn from(value: starknet_api::block::BlockSignature) -> Self {
        Self { r: Some(value.0.r.into()), s: Some(value.0.s.into()) }
    }
}

impl TryFrom<Data> for protobuf::BlockHeadersResponse {
    type Error = ProtobufResponseToDataError;

    fn try_from(data: Data) -> Result<Self, Self::Error> {
        match data {
            Data::BlockHeaderAndSignature { header, signatures } => {
                Ok(protobuf::BlockHeadersResponse {
                    header_message: Some(protobuf::block_headers_response::HeaderMessage::Header(
                        (header, signatures).into(),
                    )),
                })
            }
            Data::Fin(data_type) => match data_type {
                DataType::SignedBlockHeader => Ok(protobuf::BlockHeadersResponse {
                    header_message: Some(protobuf::block_headers_response::HeaderMessage::Fin(
                        protobuf::Fin {},
                    )),
                }),
                _ => Err(ProtobufResponseToDataError::UnsupportedDataType {
                    data_type: data_type.to_string(),
                    type_description: "BlockHeadersResponse".to_string(),
                }),
            },
            Data::StateDiff { .. } => Err(ProtobufResponseToDataError::UnsupportedDataType {
                data_type: "StateDiff".to_string(),
                type_description: "BlockHeadersResponse".to_string(),
            }),
        }
    }
}

impl TryFrom<protobuf::BlockHeadersResponse> for Data {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::BlockHeadersResponse) -> Result<Self, Self::Error> {
        match value.header_message {
            Some(protobuf::block_headers_response::HeaderMessage::Header(header)) => {
                let signed_block_header = SignedBlockHeader::try_from(header)?;
                Ok(Data::BlockHeaderAndSignature {
                    header: signed_block_header.block_header,
                    signatures: signed_block_header.signatures,
                })
            }
            Some(protobuf::block_headers_response::HeaderMessage::Fin(_)) => {
                Ok(Data::Fin(DataType::SignedBlockHeader))
            }
            None => Err(ProtobufConversionError::MissingField {
                field_description: "BlockHeadersResponse::header_message",
            }),
        }
    }
}

impl TryFrom<protobuf::BlockHeadersRequest> for InternalQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersRequest) -> Result<Self, Self::Error> {
        let value = value.iteration.ok_or(ProtobufConversionError::MissingField {
            field_description: "BlockHeadersRequest::iteration",
        })?;
        value.try_into()
    }
}

impl From<Query> for protobuf::BlockHeadersRequest {
    fn from(value: Query) -> Self {
        protobuf::BlockHeadersRequest { iteration: Some(value.into()) }
    }
}
