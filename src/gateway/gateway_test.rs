use jsonrpsee::{types::EmptyParams, ws_client::WsClientBuilder};

use crate::{
    gateway::{api::JsonRpcClient, JsonRpcServerImpl},
    starknet::BlockNumber,
    storage::components::storage_test_utils,
};

use super::*;

#[tokio::test]
async fn test_block_number() {
    let storage_reader = storage_test_utils::get_test_storage().block_storage_reader;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();
    let result: BlockNumber = module
        .call("starknet_blockNumber", EmptyParams::new())
        .await
        .unwrap();
    assert_eq!(result, BlockNumber(0));
}

#[tokio::test]
async fn test_run_server() {
    let storage_reader = storage_test_utils::get_test_storage().block_storage_reader;
    let (addr, _handle) = run_server(storage_reader).await.unwrap();
    let client = WsClientBuilder::default()
        .build(format!("ws://{:?}", addr))
        .await
        .unwrap();
    let block_number = client.block_number().await.unwrap();
    assert_eq!(block_number, BlockNumber(0));
}
