mod api;
#[cfg(test)]
mod gateway_test;
mod objects;

use std::fmt::Display;
use std::net::SocketAddr;

use jsonrpsee::core::{async_trait, Error};
use jsonrpsee::http_server::types::error::CallError;
use jsonrpsee::http_server::{HttpServerBuilder, HttpServerHandle};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::{ErrorObject, INTERNAL_ERROR_MSG};
use log::{error, info};
use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockBody, BlockNumber, ContractAddress, DeclareTransactionReceipt, GlobalRoot, StarkFelt,
    StarkHash, StateNumber, StorageKey, TransactionHash, TransactionOffsetInBlock, GENESIS_HASH,
};

use self::api::*;
use self::objects::{
    from_starknet_storage_diffs, BlockHeader, StateDiff, TransactionWithType, Transactions,
};
use crate::storage::components::{
    BlockStorageReader, BlockStorageTxn, BodyStorageReader, HeaderStorageReader, StateStorageReader,
};
use crate::storage::db::TransactionKind;

#[derive(Serialize, Deserialize)]
pub struct GatewayConfig {
    pub server_ip: String,
}

/// Rpc server.
struct JsonRpcServerImpl {
    storage_reader: BlockStorageReader,
}

impl From<JsonRpcError> for Error {
    fn from(err: JsonRpcError) -> Self {
        Error::Call(CallError::Custom(ErrorObject::owned(err as i32, err.to_string(), None::<()>)))
    }
}

fn internal_server_error(err: impl Display) -> Error {
    error!("{}: {}", INTERNAL_ERROR_MSG, err);
    Error::Call(CallError::Custom(ErrorObject::owned(
        InternalError.code(),
        INTERNAL_ERROR_MSG,
        None::<()>,
    )))
}

fn get_block_number<Mode: TransactionKind>(
    txn: &BlockStorageTxn<'_, Mode>,
    block_id: BlockId,
) -> Result<BlockNumber, Error> {
    Ok(match block_id {
        BlockId::HashOrNumber(BlockHashOrNumber::Hash(block_hash)) => txn
            .get_block_number_by_hash(&block_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?,
        BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number)) => {
            // Check that the block exists.
            let last_block_number = get_latest_block_number(txn)?
                .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?;
            if block_number.0 > last_block_number.0 {
                return Err(Error::from(JsonRpcError::InvalidBlockId));
            }
            block_number
        }
        BlockId::Tag(Tag::Latest) => get_latest_block_number(txn)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?,
        BlockId::Tag(Tag::Pending) => {
            // TODO(anatg): Support pending block.
            todo!("Pending tag is not supported yet.")
        }
    })
}

fn get_latest_block_number<Mode: TransactionKind>(
    txn: &BlockStorageTxn<'_, Mode>,
) -> Result<Option<BlockNumber>, Error> {
    Ok(txn.get_header_marker().map_err(internal_server_error)?.prev())
}

fn get_block_by_number<Mode: TransactionKind>(
    txn: &BlockStorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<(BlockHeader, BlockBody), Error> {
    let header = txn
        .get_block_header(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?;

    let transactions = txn
        .get_block_transactions(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?;

    Ok((BlockHeader::from(header), BlockBody { transactions }))
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    fn block_number(&self) -> Result<BlockNumber, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        get_latest_block_number(&txn)?.ok_or_else(|| Error::from(JsonRpcError::NoBlocks))
    }

    fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> Result<Block, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let (header, body) = get_block_by_number(&txn, block_number)?;
        let transaction_hashes: Vec<TransactionHash> =
            body.transactions.iter().map(|transaction| transaction.transaction_hash()).collect();

        Ok(Block { header, transactions: Transactions::Hashes(transaction_hashes) })
    }

    fn get_block_w_full_transactions(&self, block_id: BlockId) -> Result<Block, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let (header, body) = get_block_by_number(&txn, block_number)?;

        Ok(Block {
            header,
            transactions: Transactions::Full(
                body.transactions.into_iter().map(TransactionWithType::from).collect(),
            ),
        })
    }

    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_id: BlockId,
    ) -> Result<StarkFelt, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        // Check that the block is valid and get the state number.
        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        // Check that the contract exists.
        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))?;

        state_reader.get_storage_at(state, &contract_address, &key).map_err(internal_server_error)
    }

    fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<TransactionWithType, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let (block_number, tx_offset_in_block) = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionHash))?;

        let transaction = txn
            .get_transaction(block_number, tx_offset_in_block)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionHash))?;

        Ok(TransactionWithType::from(transaction))
    }

    fn get_transaction_by_block_id_and_index(
        &self,
        block_id: BlockId,
        index: TransactionOffsetInBlock,
    ) -> Result<TransactionWithType, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;

        let transaction = txn
            .get_transaction(block_number, index)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionIndex))?;

        Ok(TransactionWithType::from(transaction))
    }

    fn get_block_transaction_count(&self, block_id: BlockId) -> Result<usize, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;

        let transactions = txn
            .get_block_transactions(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?;

        Ok(transactions.len())
    }

    fn get_state_update(&self, block_id: BlockId) -> Result<StateUpdate, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        // Get the block header for the block hash and state root.
        let block_number = get_block_number(&txn, block_id)?;
        let header = txn
            .get_block_header(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?;

        // Get the old root.
        let parent_block_number = get_block_number(
            &txn,
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.parent_hash)),
        );
        let mut old_root =
            GlobalRoot(StarkHash::from_hex(GENESIS_HASH).map_err(internal_server_error)?);
        if parent_block_number.is_ok() {
            let parent_header = txn
                .get_block_header(parent_block_number.unwrap())
                .map_err(internal_server_error)?
                .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?;
            old_root = parent_header.state_root;
        }

        // Get the block state diff.
        let db_state_diff = txn
            .get_state_diff(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockId))?;

        Ok(StateUpdate {
            block_hash: header.block_hash,
            new_root: header.state_root,
            old_root,
            state_diff: StateDiff {
                storage_diffs: from_starknet_storage_diffs(db_state_diff.storage_diffs),
                declared_contracts: vec![],
                deployed_contracts: db_state_diff.deployed_contracts,
                nonces: vec![],
            },
        })
    }

    fn get_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<TransactionReceipt, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let (_block_number, _tx_offset_in_block) = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionHash))?;

        // TODO(anatg): Get the transaction receipt from the storage.
        Ok(TransactionReceipt::Declare(DeclareTransactionReceipt::default()))
    }

    fn get_class(&self, _class_hash: ClassHash) -> Result<ContractClass, Error> {
        // TODO(anatg): Read contract class from storage.
        Ok(ContractClass::default())
    }

    fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<ContractClass, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        let _class_hash = state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))?;

        // TODO(anatg): Read contract class from storage.
        Ok(ContractClass::default())
    }
}

pub async fn run_server(
    config: GatewayConfig,
    storage_reader: BlockStorageReader,
) -> anyhow::Result<(SocketAddr, HttpServerHandle)> {
    info!("Starting gateway.");
    let server = HttpServerBuilder::default().build(&config.server_ip).await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    info!("Gateway is running - {}.", addr);
    Ok((addr, handle))
}
