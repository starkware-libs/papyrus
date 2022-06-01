use jsonrpsee::{types::EmptyParams, ws_client::WsClientBuilder};

use crate::{
    gateway::{api::JsonRpcClient, JsonRpcServerImpl},
    starknet::BlockNumber,
    storage::create_store_access,
};

use super::*;

#[tokio::test]
async fn test_block_number() {
    let (storage_reader, _) = create_store_access().unwrap();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();
    let result: BlockNumber = module
        .call("starknet_blockNumber", EmptyParams::new())
        .await
        .unwrap();
    assert_eq!(result, BlockNumber(0));
}

#[tokio::test]
async fn test_run_server() {
    let (storage_reader, _) = create_store_access().unwrap();
    let (addr, _handle) = run_server(storage_reader).await.unwrap();
    let client = WsClientBuilder::default()
        .build(format!("ws://{:?}", addr))
        .await
        .unwrap();
    let block_number = client.block_number().await.unwrap();
    assert_eq!(block_number, BlockNumber(0));
}
