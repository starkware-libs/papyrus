use std::convert::{TryFrom, TryInto};

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

use crate::messages::{
    enum_int_to_l1_data_availability_mode,
    l1_data_availability_mode_to_enum_int,
    protobuf,
    ProtobufConversionError,
};
use crate::SignedBlockHeader;

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
                state_diff_commitment: None,
                transaction_commitment: Some(transaction_commitment),
                event_commitment: Some(event_commitment),
                n_transactions: Some(n_transactions),
                n_events: Some(n_events),
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
