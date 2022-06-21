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
use log::error;

use crate::starknet::BlockNumber;
use crate::storage::components::{BlockStorageReader, HeaderStorageReader};

use self::api::{BlockNumberOrTag, BlockResponseScope, JsonRpcError, JsonRpcServer, Tag};
use self::objects::{Block, BlockStatus, Transactions};

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
        let block_number = match block_number {
            BlockNumberOrTag::Tag(Tag::Latest) => self.block_number().await?,
            BlockNumberOrTag::Tag(Tag::Pending) => {
                // TODO(anatg): Support pending block.
                todo!("Pending tag is not supported yet.")
            }
            BlockNumberOrTag::Number(number) => number,
        };

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
            // TODO(anatg): Get the status.
            status: BlockStatus::AcceptedOnL2,
            sequencer: block_header.sequencer,
            new_root: block_header.state_root,
            // TODO(anatg): Get the old root.
            old_root: block_header.state_root,
            accepted_time: block_header.timestamp,
            // TODO(anatg): Get the transaction according to the requested scope.
            transactions: Transactions::Hashes(vec![]),
        })
    }
}

pub async fn run_server(
    storage_reader: BlockStorageReader,
) -> anyhow::Result<(SocketAddr, WsServerHandle)> {
    let server = WsServerBuilder::default().build(SERVER_IP).await?;
    let addr = server.local_addr()?;
    let handle = server.start(JsonRpcServerImpl { storage_reader }.into_rpc())?;
    Ok((addr, handle))
}
