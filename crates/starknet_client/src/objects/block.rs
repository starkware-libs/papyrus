use std::ops::Index;

use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::block::{
    Block as starknet_api_block,
    BlockHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
};
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
#[cfg(doc)]
use starknet_api::transaction::TransactionOutput as starknet_api_transaction_output;
use starknet_api::transaction::{TransactionHash, TransactionOffsetInBlock};

use crate::objects::transaction::{
    L1ToL2Message, Transaction, TransactionReceipt, TransactionType,
};
use crate::{ClientError, ClientResult};

#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Serialize, PartialOrd, Ord)]
pub struct GlobalRoot(pub StarkHash);

// TODO(dvir): remove this deserialization when all the environments use the same format.
impl<'de> Deserialize<'de> for GlobalRoot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut as_string = String::deserialize(deserializer)?;
        if !as_string.starts_with("0x") {
            as_string = format!("0x{}", as_string);
        }
        let string_des = serde::de::value::StringDeserializer::new(as_string);
        Ok(Self(Deserialize::deserialize(string_des)?))
    }
}

impl From<GlobalRoot> for starknet_api::core::GlobalRoot {
    fn from(val: GlobalRoot) -> Self {
        Self(val.0)
    }
}

/// A block as returned by the starknet gateway.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct Block {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    pub gas_price: GasPrice,
    pub parent_block_hash: BlockHash,
    #[serde(default)]
    pub sequencer_address: ContractAddress,
    pub state_root: GlobalRoot,
    pub status: BlockStatus,
    #[serde(default)]
    pub timestamp: BlockTimestamp,
    pub transactions: Vec<Transaction>,
    pub transaction_receipts: Vec<TransactionReceipt>,
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

/// Converts the client representation of [`Block`] to a [`starknet_api`][`Block`].
impl TryFrom<Block> for starknet_api::block::Block {
    type Error = ClientError;

    fn try_from(block: Block) -> ClientResult<Self> {
        // Check that the number of receipts is the same as the number of transactions.
        let num_of_txs = block.transactions.len();
        let num_of_receipts = block.transaction_receipts.len();
        if num_of_txs != num_of_receipts {
            return Err(ClientError::TransactionReceiptsError(
                TransactionReceiptsError::WrongNumberOfReceipts {
                    block_number: block.block_number,
                    num_of_txs,
                    num_of_receipts,
                },
            ));
        }

        // Get the transaction outputs.
        let mut transaction_outputs = vec![];
        for (i, receipt) in block.transaction_receipts.into_iter().enumerate() {
            let transaction = block.transactions.index(i);

            // Check that the transaction index that appears in the receipt is the same as the
            // index of the transaction.
            if i != receipt.transaction_index.0 {
                return Err(ClientError::TransactionReceiptsError(
                    TransactionReceiptsError::MismatchTransactionIndex {
                        block_number: block.block_number,
                        tx_index: TransactionOffsetInBlock(i),
                        tx_hash: transaction.transaction_hash(),
                        receipt_tx_index: receipt.transaction_index,
                    },
                ));
            }

            // Check that the transaction hash that appears in the receipt is the same as in the
            // transaction.
            if transaction.transaction_hash() != receipt.transaction_hash {
                return Err(ClientError::TransactionReceiptsError(
                    TransactionReceiptsError::MismatchTransactionHash {
                        block_number: block.block_number,
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
                return Err(ClientError::TransactionReceiptsError(
                    TransactionReceiptsError::MismatchFields {
                        block_number: block.block_number,
                        tx_index: TransactionOffsetInBlock(i),
                        tx_hash: transaction.transaction_hash(),
                        tx_type: transaction.transaction_type(),
                    },
                ));
            }

            let tx_output =
                receipt.into_starknet_api_transaction_output(transaction.transaction_type());
            transaction_outputs.push(tx_output);
        }

        // Get the transactions.
        // Note: This cannot happen before getting the transaction outputs since we need to borrow
        // the block transactions inside the for loop for the transaction type (TransactionType is
        // defined in starknet_client therefore starknet_api::Transaction cannot return it).
        let transactions: Vec<_> = block
            .transactions
            .into_iter()
            .map(starknet_api::transaction::Transaction::try_from)
            .collect::<Result<_, ClientError>>()?;

        // Get the header.
        let header = starknet_api::block::BlockHeader {
            block_hash: block.block_hash,
            parent_hash: block.parent_block_hash,
            block_number: block.block_number,
            gas_price: block.gas_price,
            state_root: block.state_root.into(),
            sequencer: block.sequencer_address,
            timestamp: block.timestamp,
        };

        let body = starknet_api::block::BlockBody { transactions, transaction_outputs };

        Ok(Self { header, body })
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
