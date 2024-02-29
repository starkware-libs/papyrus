use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPrice, GasPricePerToken};
use starknet_api::core::{GlobalRoot, SequencerContractAddress, TransactionCommitment};
use starknet_api::data_availability::L1DataAvailabilityMode;

use super::block::BlockStatus;
use super::transaction::{Transaction, TransactionReceipt};
use crate::reader::StateDiff;

#[derive(Debug, Default, Deserialize, Clone, Eq, PartialEq)]
pub struct PendingData {
    pub block: PendingBlockOrDeprecated,
    pub state_update: PendingStateUpdate,
}

#[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum PendingBlockOrDeprecated {
    Deprecated(DeprecatedPendingBlock),
    Current(PendingBlock),
}

impl Default for PendingBlockOrDeprecated {
    fn default() -> Self {
        PendingBlockOrDeprecated::Deprecated(DeprecatedPendingBlock::default())
    }
}

impl PendingBlockOrDeprecated {
    pub fn block_hash(&self) -> Option<BlockHash> {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => {
                block.accepted_on_l2_extra_data.as_ref().map(|data| data.block_hash)
            }
            PendingBlockOrDeprecated::Current(block) => {
                block.accepted_on_l2_extra_data.as_ref().map(|data| data.block_hash)
            }
        }
    }
    pub fn parent_block_hash(&self) -> BlockHash {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => block.parent_block_hash,
            PendingBlockOrDeprecated::Current(block) => block.parent_block_hash,
        }
    }
    #[cfg(any(feature = "testing", test))]
    pub fn parent_block_hash_mutable(&mut self) -> &mut BlockHash {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => &mut block.parent_block_hash,
            PendingBlockOrDeprecated::Current(block) => &mut block.parent_block_hash,
        }
    }

    pub fn sequencer_address(&self) -> SequencerContractAddress {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => block.sequencer_address,
            PendingBlockOrDeprecated::Current(block) => block.sequencer_address,
        }
    }
    #[cfg(any(feature = "testing", test))]
    pub fn sequencer_address_mutable(&mut self) -> &mut SequencerContractAddress {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => &mut block.sequencer_address,
            PendingBlockOrDeprecated::Current(block) => &mut block.sequencer_address,
        }
    }
    pub fn timestamp(&self) -> BlockTimestamp {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => block.timestamp,
            PendingBlockOrDeprecated::Current(block) => block.timestamp,
        }
    }
    #[cfg(any(feature = "testing", test))]
    pub fn timestamp_mutable(&mut self) -> &mut BlockTimestamp {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => &mut block.timestamp,
            PendingBlockOrDeprecated::Current(block) => &mut block.timestamp,
        }
    }
    pub fn transactions(&self) -> &[Transaction] {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => &block.transactions,
            PendingBlockOrDeprecated::Current(block) => &block.transactions,
        }
    }
    pub fn transactions_mutable(&mut self) -> &mut Vec<Transaction> {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => &mut block.transactions,
            PendingBlockOrDeprecated::Current(block) => &mut block.transactions,
        }
    }
    pub fn transaction_receipts(&self) -> &[TransactionReceipt] {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => &block.transaction_receipts,
            PendingBlockOrDeprecated::Current(block) => &block.transaction_receipts,
        }
    }
    pub fn transaction_receipts_mutable(&mut self) -> &mut Vec<TransactionReceipt> {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => &mut block.transaction_receipts,
            PendingBlockOrDeprecated::Current(block) => &mut block.transaction_receipts,
        }
    }
    pub fn transactions_and_receipts_mutable(
        &mut self,
    ) -> (&mut Vec<Transaction>, &mut Vec<TransactionReceipt>) {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => {
                (&mut block.transactions, &mut block.transaction_receipts)
            }
            PendingBlockOrDeprecated::Current(block) => {
                (&mut block.transactions, &mut block.transaction_receipts)
            }
        }
    }
    pub fn starknet_version(&self) -> String {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => block.starknet_version.clone(),
            PendingBlockOrDeprecated::Current(block) => block.starknet_version.clone(),
        }
    }
    #[cfg(any(feature = "testing", test))]
    pub fn starknet_version_mutable(&mut self) -> &mut String {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => &mut block.starknet_version,
            PendingBlockOrDeprecated::Current(block) => &mut block.starknet_version,
        }
    }
    pub fn l1_gas_price(&self) -> GasPricePerToken {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => GasPricePerToken {
                price_in_wei: block.eth_l1_gas_price,
                price_in_fri: block.strk_l1_gas_price,
            },
            PendingBlockOrDeprecated::Current(block) => block.l1_gas_price,
        }
    }
    #[cfg(any(feature = "testing", test))]
    pub fn set_l1_gas_price(&mut self, val: &GasPricePerToken) {
        match self {
            PendingBlockOrDeprecated::Deprecated(block) => {
                block.eth_l1_gas_price = val.price_in_wei;
                block.strk_l1_gas_price = val.price_in_fri;
            }
            PendingBlockOrDeprecated::Current(block) => block.l1_gas_price = *val,
        }
    }
    pub fn l1_data_gas_price(&self) -> GasPricePerToken {
        match self {
            // In older versions, data gas price was 0.
            PendingBlockOrDeprecated::Deprecated(_) => GasPricePerToken::default(),
            PendingBlockOrDeprecated::Current(block) => block.l1_data_gas_price,
        }
    }
    pub fn l1_da_mode(&self) -> L1DataAvailabilityMode {
        match self {
            // In older versions, all blocks were using calldata.
            PendingBlockOrDeprecated::Deprecated(_) => L1DataAvailabilityMode::Calldata,
            PendingBlockOrDeprecated::Current(block) => block.l1_da_mode,
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeprecatedPendingBlock {
    #[serde(flatten)]
    pub accepted_on_l2_extra_data: Option<AcceptedOnL2ExtraData>,
    pub parent_block_hash: BlockHash,
    pub status: BlockStatus,
    // In older versions, eth_l1_gas_price was named gas_price and there was no strk_l1_gas_price.
    #[serde(alias = "gas_price")]
    pub eth_l1_gas_price: GasPrice,
    #[serde(default)]
    pub strk_l1_gas_price: GasPrice,
    pub transactions: Vec<Transaction>,
    pub timestamp: BlockTimestamp,
    pub sequencer_address: SequencerContractAddress,
    pub transaction_receipts: Vec<TransactionReceipt>,
    pub starknet_version: String,
}

#[derive(Debug, Default, Deserialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PendingBlock {
    #[serde(flatten)]
    pub accepted_on_l2_extra_data: Option<AcceptedOnL2ExtraData>,
    pub parent_block_hash: BlockHash,
    pub status: BlockStatus,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub transactions: Vec<Transaction>,
    pub timestamp: BlockTimestamp,
    pub sequencer_address: SequencerContractAddress,
    pub transaction_receipts: Vec<TransactionReceipt>,
    pub starknet_version: String,
    pub l1_da_mode: L1DataAvailabilityMode,

    // We do not care about commitments in pending blocks.
    #[serde(default)]
    pub transaction_commitment: Option<TransactionCommitment>,
    #[serde(default)]
    pub event_commitment: Option<TransactionCommitment>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct PendingStateUpdate {
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize)]
pub struct AcceptedOnL2ExtraData {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub state_root: GlobalRoot,
}
