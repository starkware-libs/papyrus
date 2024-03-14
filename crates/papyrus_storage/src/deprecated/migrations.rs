use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPricePerToken};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use tracing::error;

use crate::db::serialization::{Migratable, StorageSerde, StorageSerdeError};
use crate::header::StorageBlockHeader;

impl Migratable for StorageBlockHeader {
    fn try_from_older_version(
        bytes: &mut impl std::io::Read,
        older_version: u8,
    ) -> Result<Self, StorageSerdeError> {
        match older_version {
            0 => {
                let v0_header = StorageBlockHeaderV0::deserialize_from(bytes)
                    .ok_or(StorageSerdeError::Migration)?;
                Ok(StorageBlockHeaderV1::from(v0_header).into())
            }
            1 => {
                let v1_header = StorageBlockHeaderV1::deserialize_from(bytes)
                    .ok_or(StorageSerdeError::Migration)?;
                Ok(v1_header.into())
            }
            _ => {
                error!(
                    "Unable to migrate stored header from version {} to current version.",
                    older_version
                );
                return Err(StorageSerdeError::Migration);
            }
        }
    }
}

impl Migratable for StorageBlockHeaderV1 {
    fn try_from_older_version(
        bytes: &mut impl std::io::Read,
        older_version: u8,
    ) -> Result<Self, StorageSerdeError> {
        if older_version != 0 {
            error!(
                "Unable to migrate stored header from version {} to current version.",
                older_version
            );
            return Err(StorageSerdeError::Migration);
        }
        let v0_header =
            StorageBlockHeaderV0::deserialize_from(bytes).ok_or(StorageSerdeError::Migration)?;
        Ok(v0_header.into())
    }
}

// Storage headers until starknet version 0.13.1.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub(crate) struct StorageBlockHeaderV0 {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub state_root: GlobalRoot,
    pub sequencer: SequencerContractAddress,
    pub timestamp: BlockTimestamp,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub transaction_commitment: TransactionCommitment,
    pub event_commitment: EventCommitment,
    pub n_transactions: usize,
    pub n_events: usize,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub(crate) struct StorageBlockHeaderV1 {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub state_root: GlobalRoot,
    pub sequencer: SequencerContractAddress,
    pub timestamp: BlockTimestamp,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub state_diff_commitment: Option<StateDiffCommitment>,
    pub transaction_commitment: Option<TransactionCommitment>,
    pub event_commitment: Option<EventCommitment>,
    pub n_transactions: Option<usize>,
    pub n_events: Option<usize>,
}

impl From<StorageBlockHeaderV0> for StorageBlockHeaderV1 {
    fn from(v0_header: StorageBlockHeaderV0) -> Self {
        // In older versions, the transaction_commitment and event_commitment are 0 instead of None.
        let missing_commitments_data = v0_header.transaction_commitment
            == TransactionCommitment::default()
            && v0_header.event_commitment == EventCommitment::default();
        Self {
            block_hash: v0_header.block_hash,
            parent_hash: v0_header.parent_hash,
            block_number: v0_header.block_number,
            l1_gas_price: v0_header.l1_gas_price,
            l1_data_gas_price: v0_header.l1_data_gas_price,
            state_root: v0_header.state_root,
            sequencer: v0_header.sequencer,
            timestamp: v0_header.timestamp,
            l1_da_mode: v0_header.l1_da_mode,
            state_diff_commitment: None,
            transaction_commitment: if missing_commitments_data {
                None
            } else {
                Some(v0_header.transaction_commitment)
            },
            event_commitment: if missing_commitments_data {
                None
            } else {
                Some(v0_header.event_commitment)
            },
            n_transactions: if missing_commitments_data {
                None
            } else {
                Some(v0_header.n_transactions)
            },
            n_events: if missing_commitments_data { None } else { Some(v0_header.n_events) },
        }
    }
}

impl From<StorageBlockHeaderV1> for StorageBlockHeader {
    fn from(v1_header: StorageBlockHeaderV1) -> Self {
        Self {
            block_hash: v1_header.block_hash,
            parent_hash: v1_header.parent_hash,
            block_number: v1_header.block_number,
            l1_gas_price: v1_header.l1_gas_price,
            l1_data_gas_price: v1_header.l1_data_gas_price,
            state_root: v1_header.state_root,
            sequencer: v1_header.sequencer,
            timestamp: v1_header.timestamp,
            l1_da_mode: v1_header.l1_da_mode,
            state_diff_commitment: v1_header.state_diff_commitment,
            state_diff_length: None,
            receipt_commitment: None,
            transaction_commitment: v1_header.transaction_commitment,
            event_commitment: v1_header.event_commitment,
            n_transactions: v1_header.n_transactions,
            n_events: v1_header.n_events,
        }
    }
}
