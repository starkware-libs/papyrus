use jsonrpsee::core::Error;
use jsonrpsee::types::EmptyParams;
use jsonrpsee::ws_client::WsClientBuilder;

use crate::storage::components::storage_test_utils;

use super::api::*;
use super::*;

#[tokio::test]
async fn test_block_number() {
    let storage_reader = storage_test_utils::get_test_storage().block_storage_reader;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();
    let err = module
        .call::<_, BlockNumber>("starknet_blockNumber", EmptyParams::new())
        .await
        .unwrap_err();
    let _expected = Error::from(api::JsonRpcError::NoBlocks);
    assert_matches!(err, _expected);
}

#[tokio::test]
async fn test_run_server() {
    let storage_reader = storage_test_utils::get_test_storage().block_storage_reader;
    let (addr, _handle) = run_server(storage_reader).await.unwrap();
    let client = WsClientBuilder::default()
        .build(format!("ws://{:?}", addr))
        .await
        .unwrap();
    let err = client.block_number().await.unwrap_err();
    let _expected = Error::from(api::JsonRpcError::NoBlocks);
    assert_matches!(err, _expected);
}
