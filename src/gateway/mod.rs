mod api;
#[cfg(test)]
mod gateway_test;
mod objects;

use std::fmt::Display;
use std::net::SocketAddr;

use jsonrpsee::core::{async_trait, Error};
use jsonrpsee::types::error::ErrorCode::InternalError;
use jsonrpsee::types::error::{ErrorObject, INTERNAL_ERROR_MSG};
use jsonrpsee::ws_server::types::error::CallError;
use jsonrpsee::ws_server::{WsServerBuilder, WsServerHandle};
use log::{error, info};

use crate::starknet::{BlockNumber, ContractAddress, StarkFelt, StateNumber, StorageKey};
use crate::storage::components::{BlockStorageReader, HeaderStorageReader, StateStorageReader};

use self::api::{
    BlockHashOrTag, BlockNumberOrTag, BlockResponseScope, JsonRpcError, JsonRpcServer, Tag,
};
use self::objects::{Block, Transactions};

// TODO(anatg): Take from config.
const SERVER_IP: &str = "127.0.0.1:0";

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

impl JsonRpcServerImpl {
    fn get_block_number_or_tag(
        &self,
        block_hash: BlockHashOrTag,
    ) -> Result<BlockNumberOrTag, Error> {
        match block_hash {
            BlockHashOrTag::Tag(Tag::Latest) => Ok(BlockNumberOrTag::Tag(Tag::Latest)),
            BlockHashOrTag::Tag(Tag::Pending) => Ok(BlockNumberOrTag::Tag(Tag::Pending)),
            BlockHashOrTag::Hash(hash) => Ok(BlockNumberOrTag::Number(
                self.storage_reader
                    .get_block_number_by_hash(&hash)
                    .map_err(internal_server_error)?
                    .ok_or_else(|| Error::from(JsonRpcError::InvalidBlockHash))?,
            )),
        }
    }

    async fn get_block_number(&self, block_number: BlockNumberOrTag) -> Result<BlockNumber, Error> {
        Ok(match block_number {
            BlockNumberOrTag::Tag(Tag::Latest) => self.block_number().await?,
            BlockNumberOrTag::Tag(Tag::Pending) => {
                // TODO(anatg): Support pending block.
                todo!("Pending tag is not supported yet.")
            }
            BlockNumberOrTag::Number(number) => number,
        })
    }
}

#[async_trait]
impl JsonRpcServer for JsonRpcServerImpl {
    async fn block_number(&self) -> Result<BlockNumber, Error> {
        self.storage_reader
            .get_header_marker()
            .map_err(internal_server_error)?
            .prev()
            .ok_or_else(|| JsonRpcError::NoBlocks.into())
    }

    async fn get_block_by_number(
        &self,
        block_number: BlockNumberOrTag,
        _requested_scope: Option<BlockResponseScope>,
    ) -> Result<Block, Error> {
        let block_number = self.get_block_number(block_number).await?;

        // TODO(anatg): Get the entire block.
        let block_header = self
            .storage_reader
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

    async fn get_block_by_hash(
        &self,
        block_hash: BlockHashOrTag,
        requested_scope: Option<BlockResponseScope>,
    ) -> Result<Block, Error> {
        let block_number = self.get_block_number_or_tag(block_hash)?;
        self.get_block_by_number(block_number, requested_scope)
            .await
    }

    async fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
        block_hash: BlockHashOrTag,
    ) -> Result<StarkFelt, Error> {
        // Check that the block is valid and get the state number.
        let block_number = self
            .get_block_number(self.get_block_number_or_tag(block_hash)?)
            .await?;
        let state = StateNumber::right_after_block(block_number);

        let statetxn = self
            .storage_reader
            .get_state_reader_txn()
            .map_err(internal_server_error)?;
        let state_reader = statetxn.get_state_reader().map_err(internal_server_error)?;

        // Check that the contract exists.
        state_reader
            .get_class_hash_at(state, &contract_address)
            .map_err(internal_server_error)?
            .ok_or_else(|| Error::from(JsonRpcError::ContractNotFound))?;

        state_reader
            .get_storage_at(state, &contract_address, &key)
            .map_err(internal_server_error)
    }
}

pub async fn run_server(
    storage_reader: BlockStorageReader,
) -> anyhow::Result<(SocketAddr, WsServerHandle)> {
    info!("Starting gateway.");
    let server = WsServerBuilder::default().build(SERVER_IP).await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    info!("Gateway is running - {}.", addr);
    Ok((addr, handle))
}
