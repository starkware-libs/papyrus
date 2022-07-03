use jsonrpsee::core::Error;
use jsonrpsee::types::EmptyParams;
use jsonrpsee::ws_client::WsClientBuilder;

use crate::starknet::{
    shash, BlockHash, BlockHeader, ClassHash, DeployedContract, StarkHash, StateDiffForward,
    StorageDiff, StorageEntry,
};
use crate::storage::components::{storage_test_utils, HeaderStorageWriter, StateStorageWriter};

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
        status: block_header.status.into(),
        sequencer: block_header.sequencer,
        new_root: block_header.state_root,
        accepted_time: block_header.timestamp,
        transactions: Transactions::Hashes(vec![]),
    };
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockByNumber",
            [BlockNumberOrTag::Tag(Tag::Latest)],
        )
        .await
        .unwrap();
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
async fn test_get_block_by_hash() {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();
    let block_hash = BlockHash(shash!(
        "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"
    ));
    let header = BlockHeader {
        block_hash,
        ..BlockHeader::default()
    };
    storage_writer
        .append_header(header.number, &header)
        .unwrap();
    let block = module
        .call::<_, Block>(
            "starknet_getBlockByHash",
            [BlockHashOrTag::Hash(block_hash)],
        )
        .await
        .unwrap();
    let block_header = &BlockHeader::default();
    let expected_block = Block {
        block_hash,
        parent_hash: block_header.parent_hash,
        block_number: block_header.number,
        status: block_header.status.into(),
        sequencer: block_header.sequencer,
        new_root: block_header.state_root,
        accepted_time: block_header.timestamp,
        transactions: Transactions::Hashes(vec![]),
    };
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockByHash",
            [BlockHashOrTag::Tag(Tag::Latest)],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for an invalid block.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockByHash",
            [BlockHashOrTag::Hash(BlockHash(shash!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            )))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockHash as i32,
        JsonRpcError::InvalidBlockHash.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn test_get_storage_at() {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let block_hash = BlockHash(shash!(
        "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"
    ));
    let header = BlockHeader {
        number: BlockNumber(0),
        block_hash,
        ..BlockHeader::default()
    };
    storage_writer
        .append_header(header.number, &header)
        .unwrap();
    let address = ContractAddress(shash!("0x11"));
    let class_hash = ClassHash(shash!("0x4"));
    let key = StorageKey(shash!("0x1001"));
    let value = shash!("0x200");
    let diff = StateDiffForward {
        deployed_contracts: vec![DeployedContract {
            address,
            class_hash,
        }],
        storage_diffs: vec![StorageDiff {
            address,
            diff: vec![StorageEntry {
                key: key.clone(),
                value,
            }],
        }],
    };
    storage_writer
        .append_state_diff(BlockNumber(0), &diff)
        .unwrap();

    let res = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (address, key.clone(), BlockHashOrTag::Hash(block_hash)),
        )
        .await
        .unwrap();
    assert_eq!(res, value);

    // Ask for an invalid contract.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                ContractAddress(shash!("0x12")),
                key.clone(),
                BlockHashOrTag::Hash(block_hash),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                address,
                key.clone(),
                BlockHashOrTag::Hash(BlockHash(shash!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                ))),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockHash as i32,
        JsonRpcError::InvalidBlockHash.to_string(),
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
