use std::collections::HashMap;

use futures::{
    channel::mpsc::{Receiver, Sender},
    StreamExt,
};
use prost::Message;
use starknet_api::{
    block::{
        BlockHash, BlockHeader, BlockNumber, BlockSignature, GasPrice, GasPricePerToken,
        StarknetVersion,
    },
    core::{EventCommitment, GlobalRoot, SequencerContractAddress, TransactionCommitment},
    crypto::Signature,
    hash::StarkHash,
};

use crate::{
    db_executor::Data,
    protobuf_messages::{
        enum_int_to_l1_data_availability_mode, l1_data_availability_mode_to_enum_int,
        protobuf::{self, ConsensusSignature},
        ProtobufConversionError,
    },
    BlockHashOrNumber, Direction, InternalQuery, Protocol, Query, ResponseReceivers,
    SignedBlockHeader,
};

impl ResponseReceivers {
    pub(crate) fn new(mut protocol_to_receiver_map: HashMap<Protocol, Receiver<Vec<u8>>>) -> Self {
        let signed_headers_receiver = protocol_to_receiver_map
            .remove(&Protocol::SignedBlockHeader)
            .expect("SignedBlockHeader receiver not found")
            .map(|data_bytes| {
                protobuf::BlockHeadersResponse::decode(&data_bytes[..])
                    .expect("failed to decode protobuf SignedBlockHeader")
                    .try_into()
                    .expect("failed to convert SignedBlockHeader")
            })
            .boxed();
        Self { signed_headers_receiver }
    }
}

#[allow(unused)]
pub struct Router {
    pub protocol_to_sender_map: HashMap<Protocol, Sender<Vec<u8>>>,
    pub protocol_to_receiver_map: Option<HashMap<Protocol, Receiver<Vec<u8>>>>,
}

impl Router {
    pub fn new(protocols: Vec<Protocol>, buffer_size: usize) -> Self {
        let mut protocol_to_sender_map = HashMap::new();
        let mut protocol_to_receiver_map = HashMap::new();
        for protocol in protocols {
            let (sender, receiver) = futures::channel::mpsc::channel(buffer_size);
            protocol_to_sender_map.insert(protocol, sender);
            protocol_to_receiver_map.insert(protocol, receiver);
        }
        Self { protocol_to_sender_map, protocol_to_receiver_map: Some(protocol_to_receiver_map) }
    }

    pub fn get_recievers(&mut self) -> HashMap<Protocol, Receiver<Vec<u8>>> {
        self.protocol_to_receiver_map.take().unwrap_or_default()
    }

    pub fn try_send(&mut self, protocol: Protocol, data: Vec<u8>) -> Result<(), ()> {
        self.protocol_to_sender_map
            .get_mut(&protocol)
            .ok_or(())
            .and_then(|sender| sender.try_send(data).map_err(|_| ()))
    }
}

impl TryFrom<protobuf::BlockHeadersResponse> for Option<SignedBlockHeader> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersResponse) -> Result<Self, Self::Error> {
        match value.header_message {
            Some(protobuf::block_headers_response::HeaderMessage::Header(header)) => {
                Ok(Some(header.try_into()?))
            }
            Some(protobuf::block_headers_response::HeaderMessage::Fin(_)) => Ok(None),
            None => Err(ProtobufConversionError::MissingField),
        }
    }
}

impl TryFrom<protobuf::SignedBlockHeader> for SignedBlockHeader {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::SignedBlockHeader) -> Result<Self, Self::Error> {
        let block_hash = value
            .block_hash
            .ok_or(ProtobufConversionError::MissingField)?
            .try_into()
            .map(BlockHash)?;

        let parent_hash = value
            .parent_hash
            .ok_or(ProtobufConversionError::MissingField)?
            .try_into()
            .map(BlockHash)?;

        let timestamp = starknet_api::block::BlockTimestamp(value.time);

        let sequencer = value
            .sequencer_address
            .ok_or(ProtobufConversionError::MissingField)?
            .try_into()
            .map(SequencerContractAddress)?;

        let state_root = value
            .state
            .and_then(|state| state.root)
            .ok_or(ProtobufConversionError::MissingField)?
            .try_into()
            .map(GlobalRoot)?;

        let n_transactions = value
            .transactions
            .as_ref()
            .ok_or(ProtobufConversionError::MissingField)?
            .n_leaves
            .try_into()
            .expect("Failed converting u64 to usize");

        let transaction_commitment = value
            .transactions
            .and_then(|transactions| transactions.root)
            .ok_or(ProtobufConversionError::MissingField)?
            .try_into()
            .map(TransactionCommitment)?;

        let n_events = value
            .events
            .as_ref()
            .ok_or(ProtobufConversionError::MissingField)?
            .n_leaves
            .try_into()
            .expect("Failed converting u64 to usize");

        let event_commitment = value
            .events
            .and_then(|events| events.root)
            .ok_or(ProtobufConversionError::MissingField)?
            .try_into()
            .map(EventCommitment)?;

        let l1_da_mode = enum_int_to_l1_data_availability_mode(value.l1_data_availability_mode)?;

        let starknet_version = StarknetVersion(value.protocol_version);

        let l1_gas_price = GasPricePerToken {
            price_in_fri: GasPrice(
                value.gas_price_fri.ok_or(ProtobufConversionError::MissingField)?.into(),
            ),
            price_in_wei: GasPrice(
                value.gas_price_wei.ok_or(ProtobufConversionError::MissingField)?.into(),
            ),
        };

        let l1_data_gas_price = GasPricePerToken {
            price_in_fri: GasPrice(
                value.data_gas_price_fri.ok_or(ProtobufConversionError::MissingField)?.into(),
            ),
            price_in_wei: GasPrice(
                value.data_gas_price_wei.ok_or(ProtobufConversionError::MissingField)?.into(),
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
                transaction_commitment,
                event_commitment,
                n_transactions,
                n_events,
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
            // TODO(shahak): fill this.
            state_diff_commitment: None,
            state: Some(protobuf::Patricia {
                // TODO(shahak): fill this.
                height: 0,
                root: Some(header.state_root.0.into()),
            }),
            transactions: Some(protobuf::Merkle {
                n_leaves: header.n_transactions.try_into().expect("Converting usize to u64 failed"),
                root: Some(header.transaction_commitment.0.into()),
            }),
            events: Some(protobuf::Merkle {
                n_leaves: header.n_events.try_into().expect("Converting usize to u64 failed"),
                root: Some(header.event_commitment.0.into()),
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
            signatures: signatures.iter().map(|sig| ConsensusSignature::from(*sig)).collect(),
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

impl From<starknet_api::block::BlockSignature> for protobuf::ConsensusSignature {
    fn from(value: starknet_api::block::BlockSignature) -> Self {
        Self { r: Some(value.0.r.into()), s: Some(value.0.s.into()) }
    }
}

impl From<Data> for protobuf::BlockHeadersResponse {
    fn from(data: Data) -> Self {
        match data {
            Data::BlockHeaderAndSignature { header, signatures } => {
                protobuf::BlockHeadersResponse {
                    header_message: Some(protobuf::block_headers_response::HeaderMessage::Header(
                        (header, signatures).into(),
                    )),
                }
            }
            Data::Fin => protobuf::BlockHeadersResponse {
                header_message: Some(protobuf::block_headers_response::HeaderMessage::Fin(
                    protobuf::Fin {},
                )),
            },
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
            Some(protobuf::block_headers_response::HeaderMessage::Fin(_)) => Ok(Data::Fin),
            None => panic!("Missing field"),
        }
    }
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

impl From<Query> for protobuf::BlockHeadersRequest {
    fn from(value: Query) -> Self {
        protobuf::BlockHeadersRequest {
            iteration: Some(protobuf::Iteration {
                direction: match value.direction {
                    Direction::Forward => 0,
                    Direction::Backward => 1,
                },
                limit: value.limit as u64,
                step: value.step as u64,
                start: Some(protobuf::iteration::Start::BlockNumber(value.start_block.0)),
            }),
        }
    }
}
