use std::convert::{TryFrom, TryInto};

use starknet_api::block::{
    BlockHash,
    BlockHeader,
    BlockNumber,
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

use crate::messages::{enum_int_to_l1_data_availability_mode, protobuf, ProtobufConversionError};
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

impl From<starknet_api::block::BlockSignature> for protobuf::ConsensusSignature {
    fn from(value: starknet_api::block::BlockSignature) -> Self {
        Self { r: Some(value.0.r.into()), s: Some(value.0.s.into()) }
    }
}
