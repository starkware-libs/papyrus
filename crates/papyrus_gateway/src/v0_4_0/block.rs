use jsonrpsee::types::ErrorObjectOwned;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::StorageTxn;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockNumber, BlockStatus, BlockTimestamp};
use starknet_api::core::{ContractAddress, GlobalRoot};

use super::transaction::Transactions;
use crate::api::JsonRpcError;
use crate::internal_server_error;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub sequencer_address: ContractAddress,
    pub new_root: GlobalRoot,
    pub timestamp: BlockTimestamp,
}

impl From<starknet_api::block::BlockHeader> for BlockHeader {
    fn from(header: starknet_api::block::BlockHeader) -> Self {
        BlockHeader {
            block_hash: header.block_hash,
            parent_hash: header.parent_hash,
            block_number: header.block_number,
            sequencer_address: header.sequencer,
            new_root: header.state_root,
            timestamp: header.timestamp,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Block {
    pub status: BlockStatus,
    #[serde(flatten)]
    pub header: BlockHeader,
    pub transactions: Transactions,
}

pub fn get_block_header_by_number<
    Mode: TransactionKind,
    BlockHeader: From<starknet_api::block::BlockHeader>,
>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<BlockHeader, ErrorObjectOwned> {
    let header = txn
        .get_block_header(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;

    Ok(BlockHeader::from(header))
}

fn get_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_id: BlockId,
) -> Result<BlockNumber, ErrorObjectOwned> {
    Ok(match block_id {
        BlockId::HashOrNumber(BlockHashOrNumber::Hash(block_hash)) => txn
            .get_block_number_by_hash(&block_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?,
        BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number)) => {
            // Check that the block exists.
            let last_block_number = get_latest_block_number(txn)?
                .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?;
            if block_number > last_block_number {
                return Err(ErrorObjectOwned::from(JsonRpcError::BlockNotFound));
            }
            block_number
        }
        BlockId::Tag(Tag::Latest) => get_latest_block_number(txn)?
            .ok_or_else(|| ErrorObjectOwned::from(JsonRpcError::BlockNotFound))?,
        BlockId::Tag(Tag::Pending) => {
            return Err(ErrorObjectOwned::from(JsonRpcError::PendingBlocksNotSupported));
        }
    })
}
