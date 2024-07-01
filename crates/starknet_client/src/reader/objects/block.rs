#[cfg(test)]
#[path = "block_test.rs"]
mod block_test;

use std::ops::Index;

use serde::{Deserialize, Serialize};
use starknet_api::block::{
    Block as starknet_api_block,
    BlockHash,
    BlockNumber,
    BlockTimestamp,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::hash::PoseidonHash;
#[cfg(doc)]
use starknet_api::transaction::TransactionOutput as starknet_api_transaction_output;
use starknet_api::transaction::{TransactionHash, TransactionOffsetInBlock};
use starknet_types_core::felt::Felt;

use crate::reader::objects::transaction::{
    L1ToL2Message,
    Transaction,
    TransactionReceipt,
    TransactionType,
};
use crate::reader::{ReaderClientError, ReaderClientResult};

/// A block as returned by the starknet gateway since V0.13.1.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BlockPostV0_13_1 {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub parent_block_hash: BlockHash,
    #[serde(default)]
    pub sequencer_address: SequencerContractAddress,
    pub state_root: GlobalRoot,
    pub status: BlockStatus,
    #[serde(default)]
    pub timestamp: BlockTimestamp,
    pub transactions: Vec<Transaction>,
    pub transaction_receipts: Vec<TransactionReceipt>,
    // Default since old blocks don't include this field.
    #[serde(default)]
    pub starknet_version: String,
    // Additions to the block structure in V0.13.1.
    pub l1_da_mode: L1DataAvailabilityMode,
    // Replacing the eth_l1_gas_price & strk_l1_gas_price fields with a single field.
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub transaction_commitment: TransactionCommitment,
    pub event_commitment: EventCommitment,
    // Additions to the block structure in V0.13.2
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_diff_commitment: Option<StateDiffCommitment>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_commitment: Option<ReceiptCommitment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_diff_length: Option<usize>,
}

impl BlockPostV0_13_1 {
    pub fn to_starknet_api_block_and_version(
        self,
        state_diff_hash: GlobalRoot,
    ) -> ReaderClientResult<starknet_api_block> {
        let block_or_deprecated = Block::PostV0_13_1(self);
        block_or_deprecated.to_starknet_api_block_and_version(state_diff_hash)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(untagged)]
pub enum Block {
    PostV0_13_1(BlockPostV0_13_1),
}

// TODO(yair): add tests for the new block.
impl Default for Block {
    fn default() -> Self {
        Block::PostV0_13_1(BlockPostV0_13_1::default())
    }
}

/// Errors that might be encountered while converting the client representation of a [`Block`] to a
/// starknet_api [Block](`starknet_api_block`), specifically when converting a list of
/// [`TransactionReceipt`] to a list of starknet_api
/// [TransactionOutput](`starknet_api_transaction_output`).
#[derive(thiserror::Error, Debug)]
pub enum TransactionReceiptsError {
    #[error(
        "In block number {} there are {} transactions and {} transaction receipts.",
        block_number,
        num_of_txs,
        num_of_receipts
    )]
    WrongNumberOfReceipts { block_number: BlockNumber, num_of_txs: usize, num_of_receipts: usize },
    #[error(
        "In block number {}, transaction in index {:?} with hash {:?} and type {:?} has a receipt \
         with mismatched fields.",
        block_number,
        tx_index,
        tx_hash,
        tx_type
    )]
    MismatchFields {
        block_number: BlockNumber,
        tx_index: TransactionOffsetInBlock,
        tx_hash: TransactionHash,
        tx_type: TransactionType,
    },
    #[error(
        "In block number {}, transaction in index {:?} with hash {:?} has a receipt with \
         transaction hash {:?}.",
        block_number,
        tx_index,
        tx_hash,
        receipt_tx_hash
    )]
    MismatchTransactionHash {
        block_number: BlockNumber,
        tx_index: TransactionOffsetInBlock,
        tx_hash: TransactionHash,
        receipt_tx_hash: TransactionHash,
    },
    #[error(
        "In block number {}, transaction in index {:?} with hash {:?} has a receipt with \
         transaction index {:?}.",
        block_number,
        tx_index,
        tx_hash,
        receipt_tx_index
    )]
    MismatchTransactionIndex {
        block_number: BlockNumber,
        tx_index: TransactionOffsetInBlock,
        tx_hash: TransactionHash,
        receipt_tx_index: TransactionOffsetInBlock,
    },
}

/// Converts the client representation of [`Block`] to a tuple of a starknet_api
/// [Block](`starknet_api_block`) and String representing the Starknet version corresponding to
/// that block.
impl Block {
    pub fn transactions(&self) -> &[Transaction] {
        match self {
            Block::PostV0_13_1(block) => &block.transactions,
        }
    }

    pub fn transaction_receipts(&self) -> &[TransactionReceipt] {
        match self {
            Block::PostV0_13_1(block) => &block.transaction_receipts,
        }
    }

    pub fn block_number(&self) -> BlockNumber {
        match self {
            Block::PostV0_13_1(block) => block.block_number,
        }
    }

    pub fn block_hash(&self) -> BlockHash {
        match self {
            Block::PostV0_13_1(block) => block.block_hash,
        }
    }

    pub fn parent_block_hash(&self) -> BlockHash {
        match self {
            Block::PostV0_13_1(block) => block.parent_block_hash,
        }
    }

    pub fn l1_gas_price(&self) -> GasPricePerToken {
        match self {
            Block::PostV0_13_1(block) => block.l1_gas_price,
        }
    }

    pub fn l1_data_gas_price(&self) -> GasPricePerToken {
        match self {
            Block::PostV0_13_1(block) => block.l1_data_gas_price,
        }
    }

    pub fn state_root(&self) -> GlobalRoot {
        match self {
            Block::PostV0_13_1(block) => block.state_root,
        }
    }

    pub fn sequencer_address(&self) -> SequencerContractAddress {
        match self {
            Block::PostV0_13_1(block) => block.sequencer_address,
        }
    }

    pub fn timestamp(&self) -> BlockTimestamp {
        match self {
            Block::PostV0_13_1(block) => block.timestamp,
        }
    }

    pub fn starknet_version(&self) -> String {
        match self {
            Block::PostV0_13_1(block) => block.starknet_version.clone(),
        }
    }

    pub fn l1_da_mode(&self) -> L1DataAvailabilityMode {
        match self {
            Block::PostV0_13_1(block) => block.l1_da_mode,
        }
    }

    pub fn transaction_commitment(&self) -> TransactionCommitment {
        match self {
            Block::PostV0_13_1(block) => block.transaction_commitment,
        }
    }

    pub fn event_commitment(&self) -> EventCommitment {
        match self {
            Block::PostV0_13_1(block) => block.event_commitment,
        }
    }

    pub fn receipt_commitment(&self) -> Option<ReceiptCommitment> {
        match self {
            Block::PostV0_13_1(block) => block.receipt_commitment,
        }
    }

    pub fn state_diff_length(&self) -> Option<usize> {
        match self {
            Block::PostV0_13_1(block) => block.state_diff_length,
        }
    }

    // TODO(shahak): Rename to to_starknet_api_block.
    pub fn to_starknet_api_block_and_version(
        self,
        // TODO(yair): Change to StateDiffCommitment.
        state_diff_commitment: GlobalRoot,
    ) -> ReaderClientResult<starknet_api_block> {
        // Check that the number of receipts is the same as the number of transactions.
        let num_of_txs = self.transactions().len();
        let num_of_receipts = self.transaction_receipts().len();
        if num_of_txs != num_of_receipts {
            return Err(ReaderClientError::TransactionReceiptsError(
                TransactionReceiptsError::WrongNumberOfReceipts {
                    block_number: self.block_number(),
                    num_of_txs,
                    num_of_receipts,
                },
            ));
        }

        let (transaction_commitment, event_commitment) = match &self {
            Block::PostV0_13_1(block) => {
                // In some older starknet versions, the transaction and event commitments are not
                // available from the feeder gateway. In such cases, we return None for these
                // fields.
                if block.transaction_commitment == TransactionCommitment::default()
                    && block.event_commitment == EventCommitment::default()
                {
                    (None, None)
                } else {
                    (Some(block.transaction_commitment), Some(block.event_commitment))
                }
            }
        };

        let n_transactions = self.transactions().len();
        let n_events =
            self.transaction_receipts().iter().fold(0, |acc, receipt| acc + receipt.events.len());

        // Get the header.
        let header = starknet_api::block::BlockHeader {
            block_hash: self.block_hash(),
            parent_hash: self.parent_block_hash(),
            block_number: self.block_number(),
            l1_gas_price: self.l1_gas_price(),
            state_root: self.state_root(),
            sequencer: self.sequencer_address(),
            timestamp: self.timestamp(),
            l1_data_gas_price: self.l1_data_gas_price(),
            l1_da_mode: self.l1_da_mode(),
            state_diff_commitment: StateDiffCommitment(PoseidonHash(state_diff_commitment.0)),
            transaction_commitment,
            event_commitment,
            receipt_commitment: self.receipt_commitment(),
            state_diff_length: self.state_diff_length(),
            n_transactions,
            n_events,
            starknet_version: StarknetVersion(self.starknet_version()),
        };

        let (transactions, transaction_receipts) = self.get_body();

        // Get the transaction outputs and execution statuses.
        let mut transaction_outputs = vec![];
        let mut transaction_hashes = vec![];
        for (i, receipt) in transaction_receipts.into_iter().enumerate() {
            let transaction = transactions.index(i);

            // Check that the transaction index that appears in the receipt is the same as the
            // index of the transaction.
            if i != receipt.transaction_index.0 {
                return Err(ReaderClientError::TransactionReceiptsError(
                    TransactionReceiptsError::MismatchTransactionIndex {
                        block_number: header.block_number,
                        tx_index: TransactionOffsetInBlock(i),
                        tx_hash: transaction.transaction_hash(),
                        receipt_tx_index: receipt.transaction_index,
                    },
                ));
            }

            // Check that the transaction hash that appears in the receipt is the same as in the
            // transaction.
            if transaction.transaction_hash() != receipt.transaction_hash {
                return Err(ReaderClientError::TransactionReceiptsError(
                    TransactionReceiptsError::MismatchTransactionHash {
                        block_number: header.block_number,
                        tx_index: TransactionOffsetInBlock(i),
                        tx_hash: transaction.transaction_hash(),
                        receipt_tx_hash: receipt.transaction_hash,
                    },
                ));
            }

            // Check that the receipt has the correct fields according to the transaction type.
            if transaction.transaction_type() != TransactionType::L1Handler
                && receipt.l1_to_l2_consumed_message != L1ToL2Message::default()
            {
                return Err(ReaderClientError::TransactionReceiptsError(
                    TransactionReceiptsError::MismatchFields {
                        block_number: header.block_number,
                        tx_index: TransactionOffsetInBlock(i),
                        tx_hash: transaction.transaction_hash(),
                        tx_type: transaction.transaction_type(),
                    },
                ));
            }

            transaction_hashes.push(receipt.transaction_hash);
            let tx_output = receipt.into_starknet_api_transaction_output(transaction);
            transaction_outputs.push(tx_output);
        }

        // Get the transactions.
        // Note: This cannot happen before getting the transaction outputs since we need to borrow
        // the block transactions inside the for loop for the transaction type (TransactionType is
        // defined in starknet_client therefore starknet_api::Transaction cannot return it).
        let transactions: Vec<_> = transactions
            .into_iter()
            .map(starknet_api::transaction::Transaction::try_from)
            .collect::<Result<_, ReaderClientError>>()?;

        let body = starknet_api::block::BlockBody {
            transactions,
            transaction_outputs,
            transaction_hashes,
        };

        Ok(starknet_api_block { header, body })
    }

    fn get_body(self) -> (Vec<Transaction>, Vec<TransactionReceipt>) {
        match self {
            Block::PostV0_13_1(block) => (block.transactions, block.transaction_receipts),
        }
    }
}

#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum BlockStatus {
    #[serde(rename(deserialize = "ABORTED", serialize = "ABORTED"))]
    Aborted,
    #[serde(rename(deserialize = "ACCEPTED_ON_L1", serialize = "ACCEPTED_ON_L1"))]
    AcceptedOnL1,
    #[serde(rename(deserialize = "ACCEPTED_ON_L2", serialize = "ACCEPTED_ON_L2"))]
    #[default]
    AcceptedOnL2,
    #[serde(rename(deserialize = "PENDING", serialize = "PENDING"))]
    Pending,
    #[serde(rename(deserialize = "REVERTED", serialize = "REVERTED"))]
    Reverted,
}

impl From<BlockStatus> for starknet_api::block::BlockStatus {
    fn from(status: BlockStatus) -> Self {
        match status {
            BlockStatus::Aborted => starknet_api::block::BlockStatus::Rejected,
            BlockStatus::AcceptedOnL1 => starknet_api::block::BlockStatus::AcceptedOnL1,
            BlockStatus::AcceptedOnL2 => starknet_api::block::BlockStatus::AcceptedOnL2,
            BlockStatus::Pending => starknet_api::block::BlockStatus::Pending,
            BlockStatus::Reverted => starknet_api::block::BlockStatus::Rejected,
        }
    }
}

/// A block signature and the input data used to create it.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum BlockSignatureData {
    Deprecated {
        block_number: BlockNumber,
        signature: [Felt; 2],
        signature_input: BlockSignatureMessage,
    },
    V0_13_2 {
        block_hash: BlockHash,
        signature: [Felt; 2],
    },
}

/// The input data used to create a block signature (Poseidon hash of this data).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockSignatureMessage {
    pub block_hash: BlockHash,
    // TODO(yair): Change to StateDiffHash.
    pub state_diff_commitment: GlobalRoot,
}
