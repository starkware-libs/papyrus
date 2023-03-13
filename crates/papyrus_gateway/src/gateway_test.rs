use std::collections::HashSet;
use std::net::SocketAddr;
use std::ops::Index;

use assert_matches::assert_matches;
use indexmap::IndexMap;
use jsonrpsee::core::Error;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::http_server::types::error::CallError;
use jsonrpsee::types::error::ErrorObject;
use jsonrpsee::types::EmptyParams;
use jsonschema::JSONSchema;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{EventIndex, TransactionIndex};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockStatus};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::StateDiff;
use starknet_api::transaction::{
    EventIndexInTransactionOutput, EventKey, Transaction, TransactionHash, TransactionOffsetInBlock,
};
use starknet_api::{patricia_key, stark_felt};
use test_utils::{
    get_rand_test_block_with_events, get_rand_test_body_with_events, get_rng, get_test_block,
    get_test_body, get_test_state_diff, send_request, GetTestInstance,
};

use crate::api::{
    BlockHashAndNumber, BlockHashOrNumber, BlockId, ContinuationToken, EventFilter, EventsChunk,
    JsonRpcClient, JsonRpcError, Tag,
};
use crate::block::Block;
use crate::state::{ContractClass, StateUpdate, ThinStateDiff};
use crate::test_utils::{
    get_starknet_spec_api_schema, get_test_gateway_config, get_test_rpc_server_and_storage_writer,
};
use crate::transaction::{
    Event, TransactionOutput, TransactionReceipt, TransactionReceiptWithStatus, TransactionStatus,
    TransactionWithType, Transactions,
};
use crate::{run_server, ContinuationTokenAsStruct};

#[tokio::test]
async fn block_number() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();

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
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .commit()
        .unwrap();
    let block_number =
        module.call::<_, BlockNumber>("starknet_blockNumber", EmptyParams::new()).await.unwrap();
    assert_eq!(block_number, BlockNumber(0));
}

#[tokio::test]
async fn block_hash_and_number() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();

    // No blocks yet.
    let err = module
        .call::<_, BlockHashAndNumber>("starknet_blockHashAndNumber", EmptyParams::new())
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    ));

    // Add a block and check again.
    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .commit()
        .unwrap();
    let block_hash_and_number = module
        .call::<_, BlockHashAndNumber>("starknet_blockHashAndNumber", EmptyParams::new())
        .await
        .unwrap();
    assert_eq!(
        block_hash_and_number,
        BlockHashAndNumber {
            block_hash: block.header.block_hash,
            block_number: block.header.block_number,
        }
    );
}

#[tokio::test]
async fn get_block_w_transaction_hashes() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();

    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = block.body.transactions.index(0);
    let expected_block = Block {
        status: BlockStatus::AcceptedOnL2,
        header: block.header.into(),
        transactions: Transactions::Hashes(vec![expected_transaction.transaction_hash()]),
    };

    // Get block by hash.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Get block by number.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>("starknet_getBlockWithTxHashes", [BlockId::Tag(Tag::Latest)])
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_block_w_full_transactions() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();

    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = block.body.transactions.index(0);
    let expected_block = Block {
        status: BlockStatus::AcceptedOnL2,
        header: block.header.into(),
        transactions: Transactions::Full(vec![expected_transaction.clone().into()]),
    };

    // Get block by hash.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Get block by number.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>("starknet_getBlockWithTxs", [BlockId::Tag(Tag::Latest)])
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_storage_at() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let header = BlockHeader::default();
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (address, storage_entries) = diff.storage_diffs.get_index(0).unwrap();
    let (key, expected_value) = storage_entries.get_index(0).unwrap();

    // Get storage by block hash.
    let res = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (*address, *key, BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Get storage by block number.
    let res = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (*address, *key, BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Ask for an invalid contract.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                ContractAddress(patricia_key!("0x12")),
                key,
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                *address,
                key,
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (*address, key, BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_class_hash_at() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let header = BlockHeader::default();
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (address, expected_class_hash) = diff.deployed_contracts.get_index(0).unwrap();

    // Get class hash by block hash.
    let res = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_class_hash);

    // Get class hash by block number.
    let res = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_class_hash);

    // Ask for an invalid contract.
    let err = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress(patricia_key!("0x12")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                *address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))), *address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_nonce() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let header = BlockHeader::default();
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (address, expected_nonce) = diff.nonces.get_index(0).unwrap();

    // Get class hash by block hash.
    let res = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_nonce);

    // Get class hash by block number.
    let res = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_nonce);

    // Ask for an invalid contract.
    let err = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress(patricia_key!("0x31")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                *address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))), *address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_transaction_by_hash() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = block.body.transactions.index(0);
    let res = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByHash",
            [expected_transaction.transaction_hash()],
        )
        .await
        .unwrap();
    assert_eq!(res, TransactionWithType::from(expected_transaction.clone()));

    // Ask for an invalid transaction.
    let err = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByHash",
            [TransactionHash(StarkHash::from(1))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::TransactionHashNotFound as i32,
        JsonRpcError::TransactionHashNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_transaction_by_block_id_and_index() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = block.body.transactions.index(0);

    // Get transaction by block hash.
    let res = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash)), 0),
        )
        .await
        .unwrap();
    assert_eq!(res, TransactionWithType::from(expected_transaction.clone()));

    // Get transaction by block number.
    let res = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(block.header.block_number)), 0),
        )
        .await
        .unwrap();
    assert_eq!(res, TransactionWithType::from(expected_transaction.clone()));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByBlockIdAndIndex",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                0,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))), 0),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid transaction index.
    let err = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash)), 1),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidTransactionIndex as i32,
        JsonRpcError::InvalidTransactionIndex.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_block_transaction_count() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let transaction_count = 5;
    let block = get_test_block(transaction_count);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body)
        .unwrap()
        .commit()
        .unwrap();

    // Get block by hash.
    let res = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(res, transaction_count);

    // Get block by number.
    let res = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(res, transaction_count);

    // Ask for the latest block.
    let res = module
        .call::<_, usize>("starknet_getBlockTransactionCount", [BlockId::Tag(Tag::Latest)])
        .await
        .unwrap();
    assert_eq!(res, transaction_count);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_state_update() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let parent_header = BlockHeader::default();
    let header = BlockHeader {
        block_hash: BlockHash(stark_felt!("0x1")),
        block_number: BlockNumber(1),
        parent_hash: parent_header.block_hash,
        ..BlockHeader::default()
    };
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_header.block_number, &parent_header)
        .unwrap()
        .append_state_diff(
            parent_header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let expected_update = StateUpdate {
        block_hash: header.block_hash,
        new_root: header.state_root,
        old_root: parent_header.state_root,
        state_diff: ThinStateDiff::from(papyrus_storage::state::data::ThinStateDiff::from(diff)),
    };

    // Get state update by block hash.
    let res = module
        .call::<_, StateUpdate>(
            "starknet_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(res, expected_update);

    // Get state update by block number.
    let res = module
        .call::<_, StateUpdate>(
            "starknet_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(res, expected_update);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, StateUpdate>(
            "starknet_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, StateUpdate>(
            "starknet_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_transaction_receipt() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let transaction = block.body.transactions.index(0);
    let output = TransactionOutput::from(block.body.transaction_outputs.index(0).clone());
    let expected_receipt = TransactionReceiptWithStatus {
        receipt: TransactionReceipt::from_transaction_output(
            output,
            transaction,
            block.header.block_hash,
            block.header.block_number,
        ),
        status: TransactionStatus::default(),
    };
    let res = module
        .call::<_, TransactionReceiptWithStatus>(
            "starknet_getTransactionReceipt",
            [transaction.transaction_hash()],
        )
        .await
        .unwrap();
    // The returned jsons of some transaction outputs are the same. When deserialized, the first
    // struct in the TransactionOutput enum that matches the json is chosen. To not depend here
    // on the order of structs we compare the serialized data.
    assert_eq!(
        serde_json::to_string(&res).unwrap(),
        serde_json::to_string(&expected_receipt).unwrap(),
    );

    // Ask for an invalid transaction.
    let err = module
        .call::<_, TransactionReceiptWithStatus>(
            "starknet_getTransactionReceipt",
            [TransactionHash(StarkHash::from(1))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::TransactionHashNotFound as i32,
        JsonRpcError::TransactionHashNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_class() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let parent_header = BlockHeader::default();
    let header = BlockHeader {
        block_hash: BlockHash(stark_felt!("0x1")),
        block_number: BlockNumber(1),
        parent_hash: parent_header.block_hash,
        ..BlockHeader::default()
    };
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_header.block_number, &parent_header)
        .unwrap()
        .append_state_diff(
            parent_header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (class_hash, contract_class) = diff.declared_classes.get_index(0).unwrap();
    let expected_contract_class = contract_class.clone().try_into().unwrap();

    // Get class by block hash.
    let res = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Ask for an invalid class hash.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ClassHash(stark_felt!("0x7")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::ClassHashNotFound as i32,
        JsonRpcError::ClassHashNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid class hash in the given block.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(parent_header.block_number)),
                *class_hash,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::ClassHashNotFound as i32,
        JsonRpcError::ClassHashNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                class_hash,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2))), *class_hash),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_class_at() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let parent_header = BlockHeader::default();
    let header = BlockHeader {
        block_hash: BlockHash(stark_felt!("0x1")),
        block_number: BlockNumber(1),
        parent_hash: parent_header.block_hash,
        ..BlockHeader::default()
    };
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_header.block_number, &parent_header)
        .unwrap()
        .append_state_diff(
            parent_header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (class_hash, contract_class) = diff.declared_classes.get_index(0).unwrap();
    let expected_contract_class = contract_class.clone().try_into().unwrap();
    assert_eq!(diff.deployed_contracts.get_index(0).unwrap().1, class_hash);
    let address = diff.deployed_contracts.get_index(0).unwrap().0;

    // Get class by block hash.
    let res = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Ask for an invalid contract.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress(patricia_key!("0x12")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid contract in the given block.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(parent_header.block_number)),
                *address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                *address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2))), *address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn chain_id() {
    let (module, _) = get_test_rpc_server_and_storage_writer();

    let res = module.call::<_, String>("starknet_chainId", EmptyParams::new()).await.unwrap();
    // The result should be equal to the result of the following python code
    // hex(int.from_bytes(b'SN_GOERLI', byteorder="big", signed=False))
    // taken from starknet documentation:
    // https://docs.starknet.io/documentation/develop/Blocks/transactions/#chain-id.
    assert_eq!(res, String::from("0x534e5f474f45524c49"));
}

#[tokio::test]
async fn get_events_chunk_size_2_with_address() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let address = ContractAddress(patricia_key!("0x22"));
    let key0 = EventKey(stark_felt!("0x6"));
    let key1 = EventKey(stark_felt!("0x7"));
    let mut rng = get_rng();
    let block = get_rand_test_block_with_events(
        &mut rng,
        2,
        5,
        Some(vec![address, ContractAddress(patricia_key!("0x23"))]),
        Some(vec![vec![key0.clone(), key1.clone(), EventKey(stark_felt!("0x8"))]]),
    );
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Create the filter: the allowed keys at index 0 are 0x6 or 0x7.
    let filter_keys = HashSet::from([key0, key1]);
    let block_id = BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number));
    let chunk_size = 2;
    let mut filter = EventFilter {
        from_block: Some(block_id),
        to_block: Some(block_id),
        continuation_token: None,
        chunk_size,
        address: Some(address),
        keys: vec![filter_keys.clone()],
    };

    // Create the events emitted from contract address 0x22 that have at least one of the allowed
    // keys at index 0.
    let block_hash = block.header.block_hash;
    let mut emitted_events = vec![];
    let mut emitted_event_indices = vec![];
    for (tx_i, tx_output) in block.body.transaction_outputs.iter().enumerate() {
        let transaction_hash = block.body.transactions.index(tx_i).transaction_hash();
        for (event_i, event) in tx_output.events().iter().enumerate() {
            if let Some(key) = event.content.keys.get(0) {
                if filter_keys.get(key).is_some() && event.from_address == address {
                    emitted_events.push(Event {
                        block_hash,
                        block_number,
                        transaction_hash,
                        event: event.clone(),
                    });
                    emitted_event_indices.push(EventIndex(
                        TransactionIndex(block_number, TransactionOffsetInBlock(tx_i)),
                        EventIndexInTransactionOutput(event_i),
                    ));
                }
            }
        }
    }

    for (i, chunk) in emitted_events.chunks(chunk_size).enumerate() {
        let res =
            module.call::<_, EventsChunk>("starknet_getEvents", [filter.clone()]).await.unwrap();
        assert_eq!(res.events, chunk);
        let index = (i + 1) * chunk_size;
        let expected_continuation_token = if index < emitted_event_indices.len() {
            Some(
                ContinuationToken::new(ContinuationTokenAsStruct(
                    *emitted_event_indices.index(index),
                ))
                .unwrap(),
            )
        } else {
            None
        };
        assert_eq!(res.continuation_token, expected_continuation_token);
        filter.continuation_token = res.continuation_token;
    }
}

#[tokio::test]
async fn get_events_chunk_size_2_without_address() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let key0 = EventKey(stark_felt!("0x6"));
    let key1 = EventKey(stark_felt!("0x7"));
    let mut rng = get_rng();
    let block = get_rand_test_block_with_events(
        &mut rng,
        2,
        5,
        None,
        Some(vec![vec![key0.clone(), key1.clone(), EventKey(stark_felt!("0x8"))]]),
    );
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Create the filter: the allowed keys at index 0 are 0x6 or 0x7.
    let filter_keys = HashSet::from([key0, key1]);
    let chunk_size = 2;
    let mut filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size,
        address: None,
        keys: vec![filter_keys.clone()],
    };

    // Create the events that have at least one of the allowed keys at index 0.
    let block_hash = block.header.block_hash;
    let mut emitted_events = vec![];
    let mut emitted_event_indices = vec![];
    for (tx_i, tx_output) in block.body.transaction_outputs.iter().enumerate() {
        let transaction_hash = block.body.transactions.index(tx_i).transaction_hash();
        for (event_i, event) in tx_output.events().iter().enumerate() {
            if let Some(key) = event.content.keys.get(0) {
                if filter_keys.get(key).is_some() {
                    emitted_events.push(Event {
                        block_hash,
                        block_number,
                        transaction_hash,
                        event: event.clone(),
                    });
                    emitted_event_indices.push(EventIndex(
                        TransactionIndex(block_number, TransactionOffsetInBlock(tx_i)),
                        EventIndexInTransactionOutput(event_i),
                    ));
                }
            }
        }
    }

    for (i, chunk) in emitted_events.chunks(chunk_size).enumerate() {
        let res =
            module.call::<_, EventsChunk>("starknet_getEvents", [filter.clone()]).await.unwrap();
        assert_eq!(res.events, chunk);
        let index = (i + 1) * chunk_size;
        let expected_continuation_token = if index < emitted_event_indices.len() {
            Some(
                ContinuationToken::new(ContinuationTokenAsStruct(
                    *emitted_event_indices.index(index),
                ))
                .unwrap(),
            )
        } else {
            None
        };
        assert_eq!(res.continuation_token, expected_continuation_token);
        filter.continuation_token = res.continuation_token;
    }
}

#[tokio::test]
async fn get_events_page_size_too_big() {
    let (module, _) = get_test_rpc_server_and_storage_writer();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: get_test_gateway_config().max_events_chunk_size + 1,
        address: None,
        keys: vec![],
    };

    let err = module.call::<_, EventsChunk>("starknet_getEvents", [filter]).await.unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::PageSizeTooBig as i32,
        JsonRpcError::PageSizeTooBig.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_events_too_many_keys() {
    let (module, _) = get_test_rpc_server_and_storage_writer();
    let keys = (0..get_test_gateway_config().max_events_keys + 1)
        .map(|i| HashSet::from([EventKey(StarkFelt::from(i as u64))]))
        .collect();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: 2,
        address: None,
        keys,
    };

    let err = module.call::<_, EventsChunk>("starknet_getEvents", [filter]).await.unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::TooManyKeysInFilter as i32,
        JsonRpcError::TooManyKeysInFilter.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_events_no_blocks() {
    let (module, _) = get_test_rpc_server_and_storage_writer();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: 2,
        address: None,
        keys: vec![],
    };

    let res = module.call::<_, EventsChunk>("starknet_getEvents", [filter]).await.unwrap();
    assert_eq!(res, EventsChunk { events: vec![], continuation_token: None });
}

#[tokio::test]
async fn get_events_no_blocks_in_filter() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let parent_block = starknet_api::block::Block::default();
    let block = starknet_api::block::Block {
        header: BlockHeader {
            parent_hash: parent_block.header.block_hash,
            block_hash: BlockHash(stark_felt!("0x1")),
            block_number: BlockNumber(1),
            ..BlockHeader::default()
        },
        body: get_test_body(1),
    };
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_block.header.block_number, &parent_block.header)
        .unwrap()
        .append_body(parent_block.header.block_number, parent_block.body)
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Create the filter.
    let filter = EventFilter {
        from_block: Some(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))),
        to_block: Some(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
        continuation_token: None,
        chunk_size: 2,
        address: None,
        keys: vec![],
    };

    let res = module.call::<_, EventsChunk>("starknet_getEvents", [filter]).await.unwrap();
    assert_eq!(res, EventsChunk { events: vec![], continuation_token: None });
}

#[tokio::test]
async fn get_events_invalid_ct() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = starknet_api::block::Block::default();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body)
        .unwrap()
        .commit()
        .unwrap();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: Some(ContinuationToken("junk".to_owned())),
        chunk_size: 2,
        address: None,
        keys: vec![],
    };

    let err = module.call::<_, EventsChunk>("starknet_getEvents", [filter]).await.unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidContinuationToken as i32,
        JsonRpcError::InvalidContinuationToken.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn run_server_no_blocks() {
    let (storage_reader, _) = get_test_storage();
    let gateway_config = get_test_gateway_config();
    let (addr, _handle) = run_server(&gateway_config, storage_reader).await.unwrap();
    let client = HttpClientBuilder::default().build(format!("http://{addr:?}")).unwrap();
    let err = client.block_number().await.unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn serialize_returns_valid_json() {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let mut rng = get_rng();
    let parent_block = starknet_api::block::Block::default();
    let block = starknet_api::block::Block {
        header: BlockHeader {
            parent_hash: parent_block.header.block_hash,
            block_hash: BlockHash(stark_felt!("0x1")),
            block_number: BlockNumber(1),
            ..BlockHeader::default()
        },
        body: get_rand_test_body_with_events(&mut rng, 5, 5, None, None),
    };
    let state_diff = StateDiff::get_test_instance(&mut rng);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_block.header.block_number, &parent_block.header)
        .unwrap()
        .append_body(parent_block.header.block_number, parent_block.body)
        .unwrap()
        .append_state_diff(parent_block.header.block_number, StateDiff::default(), IndexMap::new())
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .append_state_diff(block.header.block_number, state_diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let gateway_config = get_test_gateway_config();
    let (server_address, _handle) = run_server(&gateway_config, storage_reader).await.unwrap();

    let schema = get_starknet_spec_api_schema(&[
        "BLOCK_WITH_TXS",
        "BLOCK_WITH_TX_HASHES",
        "STATE_UPDATE",
        "CONTRACT_CLASS",
        "TXN",
        "TXN_RECEIPT",
        "EVENTS_CHUNK",
    ])
    .await;
    validate_state(&state_diff, server_address, &schema).await;
    validate_block(&block.header, server_address, &schema).await;
    validate_transaction(block.body.transactions.index(0), server_address, &schema).await;
}

async fn validate_state(state_diff: &StateDiff, server_address: SocketAddr, schema: &JSONSchema) {
    let res =
        send_request(server_address, "starknet_getStateUpdate", r#"{"block_number": 1}"#).await;
    assert!(schema.validate(&res["result"]).is_ok(), "State update is not valid.");

    let (address, _) = state_diff.deployed_contracts.get_index(0).unwrap();
    let res = send_request(
        server_address,
        "starknet_getClassAt",
        format!(r#"{{"block_number": 1}}, "0x{}""#, hex::encode(address.0.key().bytes())).as_str(),
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok(), "Class is not valid.");
}

async fn validate_block(header: &BlockHeader, server_address: SocketAddr, schema: &JSONSchema) {
    let res =
        send_request(server_address, "starknet_getBlockWithTxs", r#"{"block_number": 1}"#).await;
    assert!(schema.validate(&res["result"]).is_ok(), "Block with transactions is not valid.");

    let res = send_request(
        server_address,
        "starknet_getBlockWithTxHashes",
        format!(r#"{{"block_hash": "0x{}"}}"#, hex::encode(header.block_hash.0.bytes())).as_str(),
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok(), "Block with transaction hashes is not valid.");
}

async fn validate_transaction(tx: &Transaction, server_address: SocketAddr, schema: &JSONSchema) {
    let res = send_request(
        server_address,
        "starknet_getTransactionByBlockIdAndIndex",
        r#"{"block_number": 1}, 0"#,
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok(), "Transaction is not valid.");

    let res = send_request(
        server_address,
        "starknet_getTransactionByHash",
        format!(r#""0x{}""#, hex::encode(tx.transaction_hash().0.bytes())).as_str(),
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok(), "Transaction is not valid.");

    let res = send_request(
        server_address,
        "starknet_getTransactionReceipt",
        format!(r#""0x{}""#, hex::encode(tx.transaction_hash().0.bytes())).as_str(),
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok(), "Transaction receipt is not valid.");

    let res = send_request(server_address, "starknet_getEvents", r#"{"chunk_size": 2}"#).await;
    assert!(schema.validate(&res["result"]).is_ok(), "Events are not valid.");
}
