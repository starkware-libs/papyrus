mod api;
#[cfg(test)]
mod gateway_test;

use std::net::SocketAddr;

use jsonrpsee::{
    core::{async_trait, Error},
    http_server::{HttpServerBuilder, HttpServerHandle},
};
use log::{error, info};

use crate::{starknet::BlockNumber, storage::StorageReader};

use self::api::JsonRpcServer;

/// Rpc server.
struct Gateway {
    storage_reader: Box<dyn StorageReader>,
}

#[async_trait]
impl JsonRpcServer for Gateway {
    async fn block_number(&self) -> Result<BlockNumber, Error> {
        let res = self.storage_reader.get_latest_block_number().await;
        match res {
            Ok(block_number) => {
                info!("Read block number: {:?}.", block_number);
                Ok(block_number)
            }
            err => {
                error!("Storage error: {:?}", err);
                Err(Error::Custom(format!("Storage error: {:?}", err)))
            }
        }
    }
}

#[allow(dead_code)]
pub async fn run_server(
    storage_reader: Box<dyn StorageReader>,
) -> anyhow::Result<(SocketAddr, HttpServerHandle)> {
    let server = HttpServerBuilder::default().build("127.0.0.1:0").await?;
    let addr = server.local_addr()?;
    let handle = server.start(Gateway { storage_reader }.into_rpc())?;
    Ok((addr, handle))
}


//pub async fn run_client()