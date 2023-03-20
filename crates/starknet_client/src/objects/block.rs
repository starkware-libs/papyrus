use std::collections::HashMap;
use std::ops::Index;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
#[cfg(doc)]
use starknet_api::block::Block as starknet_api_block;
use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPrice};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::serde_utils::NonPrefixedBytesAsHex;
use starknet_api::state::{EntryPoint, EntryPointType, Program, StorageKey};
#[cfg(doc)]
use starknet_api::transaction::TransactionOutput as starknet_api_transaction_output;
use starknet_api::transaction::{TransactionHash, TransactionOffsetInBlock};
use starknet_api::StarknetApiError;

use crate::objects::transaction::{
    L1ToL2Message, Transaction, TransactionReceipt, TransactionType,
};
use crate::{ClientError, ClientResult};

#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(try_from = "NonPrefixedBytesAsHex<32_usize>")]
pub struct GlobalRoot(pub StarkHash);

// We don't use the regular StarkHash deserialization since the Starknet sequencer returns the
// global root hash as a hex string without a "0x" prefix.
impl TryFrom<NonPrefixedBytesAsHex<32_usize>> for GlobalRoot {
    type Error = StarknetApiError;
    fn try_from(val: NonPrefixedBytesAsHex<32_usize>) -> Result<Self, Self::Error> {
        Ok(Self(StarkHash::try_from(val)?))
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
        let transactions: Vec<starknet_api::transaction::Transaction> = block
            .transactions
            .into_iter()
            .map(starknet_api::transaction::Transaction::from)
            .collect();

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

/// A state update derived from a single block as returned by the starknet gateway.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct StateUpdate {
    pub block_hash: BlockHash,
    pub new_root: GlobalRoot,
    pub old_root: GlobalRoot,
    pub state_diff: StateDiff,
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

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StateDiff {
    // IndexMap is serialized as a mapping in json, keeps ordering and is efficiently iterable.
    pub storage_diffs: IndexMap<ContractAddress, Vec<StorageEntry>>,
    pub deployed_contracts: Vec<DeployedContract>,
    #[serde(default)]
    pub declared_contracts: Vec<ClassHash>,
    pub nonces: IndexMap<ContractAddress, Nonce>,
}
impl StateDiff {
    // Returns the declared class hashes and after them the deployed class hashes that weren't in
    // the declared.
    pub fn class_hashes(&self) -> Vec<ClassHash> {
        let mut deployed_class_hashes = self
            .deployed_contracts
            .iter()
            .map(|contract| contract.class_hash)
            .filter(|hash| !self.declared_contracts.contains(hash))
            .collect();
        let mut declared_class_hashes = self.declared_contracts.clone();
        declared_class_hashes.append(&mut deployed_class_hashes);
        declared_class_hashes
    }
}

/// A deployed contract in StarkNet.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployedContract {
    pub address: ContractAddress,
    pub class_hash: ClassHash,
}

/// A storage entry in a contract.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StorageEntry {
    pub key: StorageKey,
    pub value: StarkFelt,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ContractClass {
    pub abi: serde_json::Value,
    pub program: Program,
    /// The selector of each entry point is a unique identifier in the program.
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

impl From<ContractClass> for starknet_api::state::ContractClass {
    fn from(class: ContractClass) -> Self {
        // Starknet does not verify the abi. If we can't parse it, we set it to None.
        let abi = serde_json::from_value::<Vec<ContractClassAbiEntry>>(class.abi)
            .ok()
            .map(|entries| entries.into_iter().map(ContractClassAbiEntry::try_into).collect())
            .and_then(Result::ok);
        Self { abi, program: class.program, entry_points_by_type: class.entry_points_by_type }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ContractClassAbiEntry {
    Event(EventAbiEntry),
    Function(FunctionAbiEntry),
    Struct(StructAbiEntry),
}

impl ContractClassAbiEntry {
    fn try_into(self) -> Result<starknet_api::state::ContractClassAbiEntry, ()> {
        match self {
            ContractClassAbiEntry::Event(entry) => {
                Ok(starknet_api::state::ContractClassAbiEntry::Event(entry.entry))
            }
            ContractClassAbiEntry::Function(entry) => {
                Ok(starknet_api::state::ContractClassAbiEntry::Function(entry.try_into()?))
            }
            ContractClassAbiEntry::Struct(entry) => {
                Ok(starknet_api::state::ContractClassAbiEntry::Struct(entry.entry))
            }
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EventAbiEntry {
    pub r#type: String,
    #[serde(flatten)]
    pub entry: starknet_api::state::EventAbiEntry,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct FunctionAbiEntry {
    pub r#type: String,
    #[serde(flatten)]
    pub entry: starknet_api::state::FunctionAbiEntry,
}

impl FunctionAbiEntry {
    fn try_into(self) -> Result<starknet_api::state::FunctionAbiEntryWithType, ()> {
        match self.r#type.as_str() {
            "constructor" => Ok(starknet_api::state::FunctionAbiEntryType::Constructor),
            "function" => Ok(starknet_api::state::FunctionAbiEntryType::Regular),
            "l1_handler" => Ok(starknet_api::state::FunctionAbiEntryType::L1Handler),
            _ => Err(()),
        }
        .map(|t| starknet_api::state::FunctionAbiEntryWithType { r#type: t, entry: self.entry })
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct StructAbiEntry {
    pub r#type: String,
    #[serde(flatten)]
    pub entry: starknet_api::state::StructAbiEntry,
}
