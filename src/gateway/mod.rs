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

use crate::starknet::StateNumber;
use crate::storage::components::{
    BlockStorageReader, BlockStorageTxn, BodyStorageReader, HeaderStorageReader, StateStorageReader,
};
use crate::storage::db::TransactionKind;

use self::api::*;
use self::objects::Transactions;

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
        Error::Call(CallError::Custom(ErrorObject::owned(
            err as i32,
            err.to_string(),
            None::<()>,
        )))
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

fn get_block_number_or_tag<Mode: TransactionKind>(
    txn: &BlockStorageTxn<'_, Mode>,
    block_hash: BlockHashOrTag,
) -> Result<BlockNumberOrTag, Error> {
    match block_hash {
        BlockHashOrTag::Tag(Tag::Latest) => Ok(BlockNumberOrTag::Tag(Tag::Latest)),
        BlockHashOrTag::Tag(Tag::Pending) => Ok(BlockNumberOrTag::Tag(Tag::Pending)),
        BlockHashOrTag::Hash(hash) => Ok(BlockNumberOrTag::Number(
            txn.get_block_number_by_hash(&hash)
                .map_err(internal_server_error)?
                .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockHash))?,
        )),
    }
}

fn get_block_number<Mode: TransactionKind>(
    txn: &BlockStorageTxn<'_, Mode>,
    block_number: BlockNumberOrTag,
) -> Result<BlockNumber, Error> {
    Ok(match block_number {
        BlockNumberOrTag::Tag(Tag::Latest) => get_latest_block_number(txn)?,
        BlockNumberOrTag::Tag(Tag::Pending) => {
            // TODO(anatg): Support pending block.
            todo!("Pending tag is not supported yet.")
        }
        BlockNumberOrTag::Number(number) => number,
    })
}

fn get_latest_block_number<Mode: TransactionKind>(
    txn: &BlockStorageTxn<'_, Mode>,
) -> Result<BlockNumber, Error> {
    txn.get_header_marker()
        .map_err(internal_server_error)?
        .prev()
        .ok_or_else(|| JsonRpcError::NoBlocks.into())
}

fn get_block_number_from_hash<Mode: TransactionKind>(
    txn: &BlockStorageTxn<'_, Mode>,
    block_hash: BlockHashOrTag,
) -> Result<BlockNumber, Error> {
    get_block_number(txn, get_block_number_or_tag(txn, block_hash)?)
}

fn get_block_by_number<Mode: TransactionKind>(
    txn: &BlockStorageTxn<'_, Mode>,
    block_number: BlockNumber,
    _requested_scope: Option<BlockResponseScope>,
) -> Result<Block, Error> {
    // TODO(anatg): Get the entire block.
    let block_header = txn
        .get_block_header(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockNumber))?;

    Ok(Block {
        block_hash: block_header.block_hash,
        parent_hash: block_header.parent_hash,
        block_number,
        status: block_header.status.into(),
        sequencer: block_header.sequencer,
        new_root: block_header.state_root,
        accepted_time: block_header.timestamp,
        // TODO(anatg): Get the transaction according to the requested scope.
        transactions: Transactions::Hashes(vec![]),
    })
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    fn block_number(&self) -> Result<BlockNumber, Error> {
        let txn = self
            .storage_reader
            .begin_ro_txn()
            .map_err(internal_server_error)?;
        get_latest_block_number(&txn)
    }

    fn get_block_by_number(
        &self,
        block_number: BlockNumberOrTag,
        _requested_scope: Option<BlockResponseScope>,
    ) -> Result<Block, Error> {
        let txn = self
            .storage_reader
            .begin_ro_txn()
            .map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, block_number)?;
        get_block_by_number(&txn, block_number, _requested_scope)
    }

    fn get_block_by_hash(
        &self,
        block_hash: BlockHashOrTag,
        requested_scope: Option<BlockResponseScope>,
    ) -> Result<Block, Error> {
        let txn = self
            .storage_reader
            .begin_ro_txn()
            .map_err(internal_server_error)?;
        let block_number = get_block_number(&txn, get_block_number_or_tag(&txn, block_hash)?)?;

        get_block_by_number(&txn, block_number, requested_scope)
    }

    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_hash: BlockHashOrTag,
    ) -> Result<StarkFelt, Error> {
        let txn = self
            .storage_reader
            .begin_ro_txn()
            .map_err(internal_server_error)?;

        // Check that the block is valid and get the state number.
        let block_number = get_block_number_from_hash(&txn, block_hash)?;
        let state = StateNumber::right_after_block(block_number);

        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        // Check that the contract exists.
        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))?;

        state_reader
            .get_storage_at(state, &contract_address, &key)
            .map_err(internal_server_error)
    }

    fn get_transaction_by_hash(
        &self,
        transaction_hash: TransactionHash,
    ) -> Result<Transaction, Error> {
        let txn = self
            .storage_reader
            .begin_ro_txn()
            .map_err(internal_server_error)?;

        let (block_number, tx_offset_in_block) = txn
            .get_transaction_idx_by_hash(&transaction_hash)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionHash))?;

        txn.get_transaction(block_number, tx_offset_in_block)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionHash))
    }

    fn get_transaction_by_block_hash_and_index(
        &self,
        block_hash: BlockHashOrTag,
        index: TransactionOffsetInBlock,
    ) -> Result<Transaction, Error> {
        let txn = self
            .storage_reader
            .begin_ro_txn()
            .map_err(internal_server_error)?;

        let block_number = get_block_number_from_hash(&txn, block_hash)?;

        txn.get_transaction(block_number, index)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionIndex))
    }

    fn get_transaction_by_block_number_and_index(
        &self,
        block_number: BlockNumberOrTag,
        index: TransactionOffsetInBlock,
    ) -> Result<Transaction, Error> {
        let txn = self
            .storage_reader
            .begin_ro_txn()
            .map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_number)?;

        // Check that the block exists.
        let last_block_number = get_latest_block_number(&txn)?;
        if block_number.0 > last_block_number.0 {
            return Err(Error::from(JsonRpcError::InvalidBlockNumber));
        }

        txn.get_transaction(block_number, index)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::InvalidTransactionIndex))
    }

    fn get_class_hash_at(
        &self,
        block_number: BlockNumberOrTag,
        contract_address: ContractAddress,
    ) -> Result<ClassHash, Error> {
        let txn = self
            .storage_reader
            .begin_ro_txn()
            .map_err(internal_server_error)?;

        let block_number = get_block_number(&txn, block_number)?;

        // Check that the block exists.
        let last_block_number = get_latest_block_number(&txn)?;
        if block_number.0 > last_block_number.0 {
            return Err(Error::from(JsonRpcError::InvalidBlockNumber));
        }

        let state = StateNumber::right_after_block(block_number);
        let state_reader = txn.get_state_reader().map_err(internal_server_error)?;

        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))
    }
}

pub async fn run_server(
    config: GatewayConfig,
    storage_reader: BlockStorageReader,
) -> anyhow::Result<(SocketAddr, HttpServerHandle)> {
    info!("Starting gateway.");
    let server = HttpServerBuilder::default()
        .build(&config.server_ip)
        .await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    info!("Gateway is running - {}.", addr);
    Ok((addr, handle))
}
