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
use papyrus_storage::{
    BodyStorageReader, HeaderStorageReader, StateStorageReader, StorageReader, StorageTxn,
    TransactionIndex, TransactionKind,
};
use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockNumber, BlockStatus, ClassHash, ContractAddress, ContractClass, GlobalRoot, Nonce,
    StarkFelt, StarkHash, StateNumber, StorageKey, TransactionHash, TransactionOffsetInBlock,
    GENESIS_HASH,
};

use self::api::{BlockHashAndNumber, BlockHashOrNumber, BlockId, JsonRpcError, JsonRpcServer, Tag};
use self::objects::{
    Block, BlockHeader, StateUpdate, Transaction, TransactionReceipt, TransactionReceiptWithStatus,
    TransactionStatus, TransactionWithType, Transactions,
};

#[derive(Serialize, Deserialize)]
pub struct GatewayConfig {
    pub server_ip: String,
}

/// Rpc server.
struct JsonRpcServerImpl {
    storage_reader: StorageReader,
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
    txn: &StorageTxn<'_, Mode>,
    block_id: BlockId,
) -> Result<BlockNumber, Error> {
    Ok(match block_id {
        BlockId::HashOrNumber(BlockHashOrNumber::Hash(block_hash)) => txn
            .get_block_number_by_hash(&block_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?,
        BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number)) => {
            // Check that the block exists.
            let last_block_number = get_latest_block_number(txn)?
                .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?;
            if block_number > last_block_number {
                return Err(Error::from(JsonRpcError::BlockNotFound));
            }
            block_number
        }
        BlockId::Tag(Tag::Latest) => {
            get_latest_block_number(txn)?.ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?
        }
        BlockId::Tag(Tag::Pending) => {
            // TODO(anatg): Support pending block.
            todo!("Pending tag is not supported yet.")
        }
    })
}

fn get_latest_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
) -> Result<Option<BlockNumber>, Error> {
    Ok(txn.get_header_marker().map_err(internal_server_error)?.prev())
}

fn get_block_header_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<BlockHeader, Error> {
    let header = txn
        .get_block_header(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?;

    Ok(BlockHeader::from(header))
}

fn get_block_txs_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<Vec<Transaction>, Error> {
    let transactions = txn
        .get_block_transactions(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?;

    Ok(transactions.into_iter().map(Transaction::from).collect())
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    fn block_number(&self) -> Result<BlockNumber, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        get_latest_block_number(&txn)?.ok_or_else(|| Error::from(JsonRpcError::NoBlocks))
    }

    fn block_hash_and_number(&self) -> Result<BlockHashAndNumber, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number =
            get_latest_block_number(&txn)?.ok_or_else(|| Error::from(JsonRpcError::NoBlocks))?;
        let header = get_block_header_by_number(&txn, block_number)?;

        Ok(BlockHashAndNumber { block_hash: header.block_hash, block_number })
    }

    fn get_block_w_transaction_hashes(&self, block_id: BlockId) -> Result<Block, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let header = get_block_header_by_number(&txn, block_number)?;
        let transactions = get_block_txs_by_number(&txn, block_number)?;
        let transaction_hashes: Vec<TransactionHash> =
            transactions.iter().map(|transaction| transaction.transaction_hash()).collect();

        Ok(Block {
            status: BlockStatus::default(),
            header,
            transactions: Transactions::Hashes(transaction_hashes),
        })
    }

    fn get_block_w_full_transactions(&self, block_id: BlockId) -> Result<Block, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let header = get_block_header_by_number(&txn, block_number)?;
        let transactions = get_block_txs_by_number(&txn, block_number)?;

        Ok(Block {
            status: BlockStatus::default(),
            header,
            transactions: Transactions::Full(
                transactions.into_iter().map(TransactionWithType::from).collect(),
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

        let transaction_index = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        let transaction = txn
            .get_transaction(transaction_index)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

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
            .get_transaction(TransactionIndex(block_number, index))
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionIndex))?;

        Ok(TransactionWithType::from(transaction))
    }

    fn get_block_transaction_count(&self, block_id: BlockId) -> Result<usize, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_id)?;
        let transactions = get_block_txs_by_number(&txn, block_number)?;

        Ok(transactions.len())
    }

    fn get_state_update(&self, block_id: BlockId) -> Result<StateUpdate, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        // Get the block header for the block hash and state root.
        let block_number = get_block_number(&txn, block_id)?;
        let header = get_block_header_by_number(&txn, block_number)?;

        // Get the old root.
        let parent_block_number = get_block_number(
            &txn,
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.parent_hash)),
        );
        let mut old_root =
            GlobalRoot::new(StarkHash::from_hex(GENESIS_HASH).map_err(internal_server_error)?);
        if parent_block_number.is_ok() {
            let parent_header = get_block_header_by_number(&txn, parent_block_number.unwrap())?;
            old_root = parent_header.new_root;
        }

        // Get the block state diff.
        let db_state_diff = txn
            .get_state_diff(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::BlockNotFound))?;

        Ok(StateUpdate {
            block_hash: header.block_hash,
            new_root: header.new_root,
            old_root,
            state_diff: db_state_diff.into(),
        })
    }

    fn get_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<TransactionReceiptWithStatus, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let TransactionIndex(block_number, tx_offset_in_block) = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        let header =
            get_block_header_by_number(&txn, block_number).map_err(internal_server_error)?;

        let tx_output = txn
            .get_transaction_output(TransactionIndex(block_number, tx_offset_in_block))
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::TransactionHashNotFound))?;

        Ok(TransactionReceiptWithStatus {
            receipt: TransactionReceipt {
                transaction_hash,
                block_hash: header.block_hash,
                block_number,
                output: tx_output.into(),
            },
            status: TransactionStatus::default(),
        })
    }

    fn get_class(&self, block_id: BlockId, class_hash: ClassHash) -> Result<ContractClass, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        // TODO(anatg): Change the program in the class definition to the rpc api expected format.
        state_reader
            .get_class_definition_at(state_number, &class_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ClassHashNotFound))
    }

    fn get_class_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<ContractClass, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state_number = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        let class_hash = state_reader
            .get_class_hash_at(state_number, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))?;

        // TODO(anatg): Change the program in the class definition to the rpc api expected format.
        state_reader
            .get_class_definition_at(state_number, &class_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))
    }

    fn get_class_hash_at(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))
    }

    fn get_nonce(
        &self,
        block_id: BlockId,
        contract_address: ContractAddress,
    ) -> Result<Nonce, Error> {
        let txn = self.storage_reader.begin_ro_txn().map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_id)?;
        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        state_reader
            .get_nonce_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))
    }
}

pub async fn run_server(
    config: GatewayConfig,
    storage_reader: StorageReader,
) -> anyhow::Result<(SocketAddr, HttpServerHandle)> {
    info!("Starting gateway.");
    let server = HttpServerBuilder::default().build(&config.server_ip).await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    info!("Gateway is running - {}.", addr);
    Ok((addr, handle))
}
