use jsonrpsee::core::Error;
use jsonrpsee::types::EmptyParams;
use jsonrpsee::ws_client::WsClientBuilder;

use crate::starknet::BlockHeader;
use crate::storage::components::{storage_test_utils, HeaderStorageWriter};

use super::api::*;
use super::*;

#[tokio::test]
async fn test_block_number() {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    // No blocks yet.
    let err = module
        .call::<_, BlockNumber>("starknet_blockNumber", EmptyParams::new())
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    ));

    // Add a block and check again.
    storage_writer
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap();
    let block_number = module
        .call::<_, BlockNumber>("starknet_blockNumber", EmptyParams::new())
        .await
        .unwrap();
    assert_eq!(block_number, BlockNumber(0));
}

#[tokio::test]
async fn test_get_block_by_number() {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();
    storage_writer
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap();
    let block = module
        .call::<_, Block>(
            "starknet_getBlockByNumber",
            [BlockNumberOrTag::Number(BlockNumber(0))],
        )
        .await
        .unwrap();
    let block_header = &BlockHeader::default();
    let expected_block = Block {
        block_hash: block_header.block_hash,
        parent_hash: block_header.parent_hash,
        block_number: BlockNumber(0),
        status: BlockStatus::AcceptedOnL2,
        sequencer: block_header.sequencer,
        new_root: block_header.state_root,
        old_root: block_header.state_root,
        accepted_time: block_header.timestamp,
        transactions: Transactions::Hashes(vec![]),
    };
    assert_eq!(block, expected_block);

    // Ask for an invalid block.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockByNumber",
            [BlockNumberOrTag::Number(BlockNumber(1))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockNumber as i32,
        JsonRpcError::InvalidBlockNumber.to_string(),
        None::<()>,
    ));
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
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    ));
}
