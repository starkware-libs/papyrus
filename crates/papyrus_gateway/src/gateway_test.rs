use std::collections::HashSet;
use std::net::SocketAddr;
use std::ops::Index;

use assert_matches::assert_matches;
use jsonrpsee::core::Error;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::http_server::types::error::CallError;
use jsonrpsee::types::error::ErrorObject;
use jsonrpsee::types::EmptyParams;
use jsonschema::JSONSchema;
use papyrus_storage::test_utils::{get_test_block, get_test_state_diff, get_test_storage};
use papyrus_storage::{
    BodyStorageWriter, EventIndex, HeaderStorageWriter, StateStorageWriter, TransactionIndex,
};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockStatus};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::StateDiff;
use starknet_api::transaction::{
    EventIndexInTransactionOutput, EventKey, TransactionHash, TransactionOffsetInBlock,
};
use starknet_api::{patky, shash};

use crate::api::{
    BlockHashAndNumber, BlockHashOrNumber, BlockId, ContinuationToken, EventFilter, JsonRpcClient,
    JsonRpcError, Tag,
};
use crate::block::Block;
use crate::state::{ContractClass, StateUpdate, ThinStateDiff};
use crate::test_utils::{
    get_block_to_match_json_file, get_starknet_spec_api_schema, get_test_gateway_config,
    get_test_rpc_server_and_storage_writer, send_request,
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
        status: BlockStatus::default(),
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
        status: BlockStatus::default(),
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
    let (header, _, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)
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
                ContractAddress(patky!("0x12")),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
    let (header, _, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)
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
                ContractAddress(patky!("0x12")),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
    let (header, _, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)
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
                ContractAddress(patky!("0x31")),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
    let (parent_header, header, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_header.block_number, &parent_header)
        .unwrap()
        .append_state_diff(
            parent_header.block_number,
            starknet_api::state::StateDiff::default(),
            vec![],
        )
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)
        .unwrap()
        .commit()
        .unwrap();

    let expected_update = StateUpdate {
        block_hash: header.block_hash,
        new_root: header.state_root,
        old_root: parent_header.state_root,
        state_diff: ThinStateDiff::from(papyrus_storage::ThinStateDiff::from(diff)),
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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

    let transaction_hash = block.body.transactions.index(0).transaction_hash();
    let output = TransactionOutput::from(block.body.transaction_outputs.index(0).clone());
    let expected_receipt = TransactionReceiptWithStatus {
        receipt: TransactionReceipt {
            transaction_hash,
            r#type: output.r#type(),
            block_hash: block.header.block_hash,
            block_number: block.header.block_number,
            output,
        },
        status: TransactionStatus::default(),
    };
    let res = module
        .call::<_, TransactionReceiptWithStatus>(
            "starknet_getTransactionReceipt",
            [transaction_hash],
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
    let (parent_header, header, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_header.block_number, &parent_header)
        .unwrap()
        .append_state_diff(
            parent_header.block_number,
            starknet_api::state::StateDiff::default(),
            vec![],
        )
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)
        .unwrap()
        .commit()
        .unwrap();

    let (class_hash, contract_class) = diff.declared_classes.get_index(1).unwrap();
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
                ClassHash(shash!("0x7")),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
    let (parent_header, header, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_header.block_number, &parent_header)
        .unwrap()
        .append_state_diff(
            parent_header.block_number,
            starknet_api::state::StateDiff::default(),
            vec![],
        )
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(
            header.block_number,
            diff.clone(),
            deployed_contract_class_definitions.clone(),
        )
        .unwrap()
        .commit()
        .unwrap();

    let (address, hash) = diff.deployed_contracts.get_index(1).unwrap();
    let expected_contract_class = deployed_contract_class_definitions
        .iter()
        .find(|(h, _)| h == hash)
        .unwrap()
        .1
        .clone()
        .try_into()
        .unwrap();

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
                ContractAddress(patky!("0x12")),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
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
async fn get_6_events_chunk_size_2_with_address() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = get_test_block(2);
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

    // Create the filter. The allowed keys at index 0 are 0x7 or 0x6.
    let filter_keys = HashSet::from([EventKey(shash!("0x7")), EventKey(shash!("0x6"))]);
    let block_id = BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number));
    let chunk_size = 2;
    let mut filter = EventFilter {
        from_block: Some(block_id),
        to_block: Some(block_id),
        continuation_token: None,
        chunk_size,
        address: Some(ContractAddress(patky!("0x22"))),
        keys: vec![filter_keys],
    };

    // Create the events emitted from contract address 0x22 that have at least one of the allowed
    // keys at index 0.
    let event0 = block.body.transaction_outputs.index(0).events().index(0);
    let event1 = block.body.transaction_outputs.index(0).events().index(1);
    let event4 = block.body.transaction_outputs.index(0).events().index(4);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber(0);
    let tx_hash1 = TransactionHash(StarkHash::from(0));
    let tx_hash3 = TransactionHash(StarkHash::from(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event1.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event4.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event1.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event4.clone() },
    ];
    let mut emitted_events_iter = emitted_events.chunks(chunk_size);

    // Create the expected continuation token.
    let expected_continuation_token0 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(4),
        )))
        .unwrap();
    let expected_continuation_token1 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(1)),
            EventIndexInTransactionOutput(1),
        )))
        .unwrap();

    // Get first chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, Some(expected_continuation_token0));

    // Get second chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, Some(expected_continuation_token1));

    // Get third chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, None);
}

#[tokio::test]
async fn get_2_events_chunk_size_2_with_address() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = get_test_block(2);
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

    // Create the filter. The allowed key at index 1 is 0x6.
    let filter_keys = HashSet::from([EventKey(shash!("0x6"))]);
    let chunk_size = 2;
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size,
        address: Some(ContractAddress(patky!("0x22"))),
        keys: vec![HashSet::new(), filter_keys],
    };

    // Create the events emitted from contract address 0x22 that have at least one of the allowed
    // keys at index 0.
    let event0 = block.body.transaction_outputs.index(0).events().index(0);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber(0);
    let tx_hash1 = TransactionHash(StarkHash::from(0));
    let tx_hash3 = TransactionHash(StarkHash::from(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
    ];
    let mut emitted_events_iter = emitted_events.chunks(chunk_size);

    // Get the only chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, None);
}

#[tokio::test]
async fn get_4_events_chunk_size_3_with_address() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = get_test_block(2);
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

    // Create the filter. The allowed keys at index 0 are 0x7 or 0x9.
    let filter_keys = HashSet::from([EventKey(shash!("0x7")), EventKey(shash!("0x9"))]);
    let block_id = BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number));
    let chunk_size = 3;
    let mut filter = EventFilter {
        from_block: Some(block_id),
        to_block: None,
        continuation_token: None,
        chunk_size,
        address: Some(ContractAddress(patky!("0x22"))),
        keys: vec![filter_keys],
    };

    // Create the events emitted from contract address 0x22 that have at least one of the allowed
    // keys at index 0.
    let event0 = block.body.transaction_outputs.index(0).events().index(0);
    let event3 = block.body.transaction_outputs.index(0).events().index(3);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber(0);
    let tx_hash1 = TransactionHash(StarkHash::from(0));
    let tx_hash3 = TransactionHash(StarkHash::from(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event3.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event3.clone() },
    ];
    let mut emitted_events_iter = emitted_events.chunks(chunk_size);

    // Create the expected continuation token.
    let expected_continuation_token0 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(1)),
            EventIndexInTransactionOutput(3),
        )))
        .unwrap();

    // Get first chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, Some(expected_continuation_token0));

    // Get second chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, None);
}

#[tokio::test]
async fn get_6_events_chunk_size_2_without_address() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = get_test_block(2);
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

    // Create the filter. The allowed keys at index 0 are 0x7 or 0x9.
    let filter_keys = HashSet::from([EventKey(shash!("0x7")), EventKey(shash!("0x9"))]);
    let chunk_size = 2;
    let mut filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size,
        address: None,
        keys: vec![filter_keys],
    };

    // Create the events that have at least one of the allowed keys at index 0.
    let event0 = block.body.transaction_outputs.index(0).events().index(0);
    let event2 = block.body.transaction_outputs.index(0).events().index(2);
    let event3 = block.body.transaction_outputs.index(0).events().index(3);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber(0);
    let tx_hash1 = TransactionHash(StarkHash::from(0));
    let tx_hash3 = TransactionHash(StarkHash::from(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event2.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event3.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event2.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event3.clone() },
    ];
    let mut emitted_events_iter = emitted_events.chunks(chunk_size);

    // Create the expected continuation token.
    let expected_continuation_token0 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(3),
        )))
        .unwrap();
    let expected_continuation_token1 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(1)),
            EventIndexInTransactionOutput(2),
        )))
        .unwrap();

    // Get first chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, Some(expected_continuation_token0));

    // Get second chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, Some(expected_continuation_token1));

    // Get third chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, None);
}

#[tokio::test]
async fn get_6_events_chunk_size_4_without_address() {
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer();
    let block = get_test_block(2);
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

    // Create the filter. The allowed keys at index 0 are 0x7 or 0x9.
    let filter_keys = HashSet::from([EventKey(shash!("0x7")), EventKey(shash!("0x9"))]);
    let chunk_size = 4;
    let mut filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size,
        address: None,
        keys: vec![filter_keys],
    };

    // Create the events that have at least one of the allowed keys at index 0.
    let event0 = block.body.transaction_outputs.index(0).events().index(0);
    let event2 = block.body.transaction_outputs.index(0).events().index(2);
    let event3 = block.body.transaction_outputs.index(0).events().index(3);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber(0);
    let tx_hash1 = TransactionHash(StarkHash::from(0));
    let tx_hash3 = TransactionHash(StarkHash::from(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event2.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event3.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event2.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event3.clone() },
    ];
    let mut emitted_events_iter = emitted_events.chunks(chunk_size);

    // Create the expected continuation token.
    let expected_continuation_token0 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(1)),
            EventIndexInTransactionOutput(2),
        )))
        .unwrap();

    // Get first chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, Some(expected_continuation_token0));

    // Get second chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter])
        .await
        .unwrap();
    assert_eq!(res, emitted_events_iter.next().unwrap());
    assert_eq!(continuation_token, None);
}

#[tokio::test]
async fn run_server_scneario() {
    let (storage_reader, _) = get_test_storage();
    let gateway_config = get_test_gateway_config();
    let (addr, _handle) = run_server(&gateway_config, storage_reader).await.unwrap();
    let client = HttpClientBuilder::default().build(format!("http://{:?}", addr)).unwrap();
    let err = client.block_number().await.unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn serialize_returns_valid_json() {
    // TODO(anatg): Use the papyrus_node/main.rs, when it has configuration for running different
    // components, for openning the storage and running the server.
    let (storage_reader, mut storage_writer) = get_test_storage();
    let block0 = get_test_block(0);
    let block1 = get_block_to_match_json_file();
    let (_, _, state_diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block0.header.block_number, &block0.header)
        .unwrap()
        .append_body(block0.header.block_number, block0.body)
        .unwrap()
        .append_state_diff(block0.header.block_number, StateDiff::default(), vec![])
        .unwrap()
        .append_header(block1.header.block_number, &block1.header)
        .unwrap()
        .append_body(block1.header.block_number, block1.body)
        .unwrap()
        .append_state_diff(
            block1.header.block_number,
            state_diff,
            deployed_contract_class_definitions,
        )
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
    ])
    .await;
    validate_state(server_address, &schema).await;
    validate_block(server_address, &schema).await;
    validate_transaction(server_address, &schema).await;
}

async fn validate_state(server_address: SocketAddr, schema: &JSONSchema) {
    let res =
        send_request(server_address, "starknet_getStateUpdate", r#"{"block_number": 1}"#).await;
    assert!(schema.validate(&res["result"]).is_ok());

    let res = send_request(
        server_address,
        "starknet_getClassAt",
        r#"{"block_number": 1}, "0x543e54f26ae33686f57da2ceebed98b340c3a78e9390931bd84fb711d5caabc""#,
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok());
}

async fn validate_block(server_address: SocketAddr, schema: &JSONSchema) {
    let res =
        send_request(server_address, "starknet_getBlockWithTxs", r#"{"block_number": 1}"#).await;
    assert!(schema.validate(&res["result"]).is_ok());

    let res = send_request(
        server_address,
        "starknet_getBlockWithTxHashes",
        r#"{"block_hash": "0x75e00250d4343326f322e370df4c9c73c7be105ad9f532eeb97891a34d9e4a5"}"#,
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok());
}

async fn validate_transaction(server_address: SocketAddr, schema: &JSONSchema) {
    let res = send_request(
        server_address,
        "starknet_getTransactionByBlockIdAndIndex",
        r#"{"block_number": 1}, 0"#,
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok());

    let res = send_request(
        server_address,
        "starknet_getTransactionByHash",
        r#""0x4dd12d3b82c3d0b216503c6abf63f1ccad222461582eac82057d46c327331d2""#,
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok());

    let res = send_request(
        server_address,
        "starknet_getTransactionReceipt",
        r#""0x6525d9aa309e5c80abbdafcc434d53202e06866597cd6dbbc91e5894fad7155""#,
    )
    .await;
    assert!(schema.validate(&res["result"]).is_ok());
}
