use std::collections::HashSet;
use std::net::SocketAddr;
use std::ops::Index;

use assert_matches::assert_matches;
use jsonrpsee::core::Error;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::http_server::types::error::CallError;
use jsonrpsee::types::error::ErrorObject;
use jsonrpsee::types::EmptyParams;
use papyrus_storage::test_utils::{
    get_alpha4_starknet_block, get_test_block, get_test_state_diff, get_test_storage,
    read_json_file,
};
use papyrus_storage::{
    BodyStorageWriter, EventIndex, HeaderStorageWriter, StateStorageWriter, TransactionIndex,
};
use starknet_api::{
    shash, BlockHash, BlockHeader, BlockNumber, BlockStatus, ClassHash, ContractAddress,
    EventIndexInTransactionOutput, EventKey, Nonce, StarkFelt, StarkHash, StateDiff,
    TransactionHash, TransactionOffsetInBlock,
};

use super::api::{
    BlockHashAndNumber, BlockHashOrNumber, BlockId, ContinuationToken, EventFilter, JsonRpcClient,
    JsonRpcError, JsonRpcServer, Tag,
};
use super::objects::{
    Block, ContractClass, Event, StateUpdate, TransactionReceipt, TransactionReceiptWithStatus,
    TransactionStatus, TransactionWithType, Transactions,
};
use super::test_utils::{get_test_chain_id, get_test_gateway_config, send_request};
use super::{run_server, ContinuationTokenAsStruct, JsonRpcServerImpl};

#[tokio::test]
async fn block_number() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl {
        chain_id,
        storage_reader,
        max_events_chunk_size: 10,
        max_events_keys: 10,
    }.into_rpc();

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
        .begin_rw_txn()?
        .append_header(BlockNumber::new(0), &BlockHeader::default())?
        .commit()?;
    let block_number =
        module.call::<_, BlockNumber>("starknet_blockNumber", EmptyParams::new()).await?;
    assert_eq!(block_number, BlockNumber::new(0));
    Ok(())
}

#[tokio::test]
async fn block_hash_and_number() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

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
        .begin_rw_txn()?
        .append_header(block.header.block_number, &block.header)?
        .commit()?;
    let block_hash_and_number = module
        .call::<_, BlockHashAndNumber>("starknet_blockHashAndNumber", EmptyParams::new())
        .await?;
    assert_eq!(
        block_hash_and_number,
        BlockHashAndNumber {
            block_hash: block.header.block_hash,
            block_number: block.header.block_number,
        }
    );
    Ok(())
}

#[tokio::test]
async fn get_block_w_transaction_hashes() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()?
        .append_header(block.header.block_number, &block.header)?
        .append_body(block.header.block_number, block.body.clone())?
        .commit()?;

    let expected_transaction = block.body.transactions().index(0);
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
        .await?;
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>("starknet_getBlockWithTxHashes", [BlockId::Tag(Tag::Latest)])
        .await?;
    assert_eq!(block, expected_block);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn get_block_w_full_transactions() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()?
        .append_header(block.header.block_number, &block.header)?
        .append_body(block.header.block_number, block.body.clone())?
        .commit()?;

    let expected_transaction = block.body.transactions().index(0);
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
        .await?;
    assert_eq!(block, expected_block);

    // Get block by number.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block.header.block_number))],
        )
        .await?;
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block =
        module.call::<_, Block>("starknet_getBlockWithTxs", [BlockId::Tag(Tag::Latest)]).await?;
    assert_eq!(block, expected_block);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn get_storage_at() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();


    let (header, _, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)?
        .commit()?;

    let storage_diff = diff.storage_diffs().index(0);
    let address = storage_diff.address;
    let storage_entry = storage_diff.storage_entries().index(0);
    let key = storage_entry.key;
    let expected_value = storage_entry.value;

    // Get storage by block hash.
    let res = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (address, key, BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
        )
        .await?;
    assert_eq!(res, expected_value);

    // Get storage by block number.
    let res = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (address, key, BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
        )
        .await?;
    assert_eq!(res, expected_value);

    // Ask for an invalid contract.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                ContractAddress::try_from(shash!("0x12")).unwrap(),
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
                address,
                key,
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
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
            (address, key, BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(1)))),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn get_class_hash_at() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let (header, _, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)?
        .commit()?;

    let contract = diff.deployed_contracts().index(0);
    let address = contract.address;
    let expected_class_hash = contract.class_hash;

    // Get class hash by block hash.
    let res = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), address),
        )
        .await?;
    assert_eq!(res, expected_class_hash);

    // Get class hash by block number.
    let res = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), address),
        )
        .await?;
    assert_eq!(res, expected_class_hash);

    // Ask for an invalid contract.
    let err = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress::try_from(shash!("0x12")).unwrap(),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                address,
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
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(1))), address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn get_nonce() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let (header, _, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)?
        .commit()?;

    let contract_nonce = diff.nonces().index(0);
    let address = contract_nonce.contract_address;
    let expected_nonce = contract_nonce.nonce;

    // Get class hash by block hash.
    let res = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), address),
        )
        .await?;
    assert_eq!(res, expected_nonce);

    // Get class hash by block number.
    let res = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), address),
        )
        .await?;
    assert_eq!(res, expected_nonce);

    // Ask for an invalid contract.
    let err = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress::try_from(shash!("0x31")).unwrap(),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                address,
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
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(1))), address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn get_transaction_by_hash() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()?
        .append_body(block.header.block_number, block.body.clone())?
        .commit()?;

    let expected_transaction = block.body.transactions().index(0);
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
            [TransactionHash(StarkHash::from_u64(1))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::TransactionHashNotFound as i32,
        JsonRpcError::TransactionHashNotFound.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn get_transaction_by_block_id_and_index() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()?
        .append_header(block.header.block_number, &block.header)?
        .append_body(block.header.block_number, block.body.clone())?
        .commit()?;

    let expected_transaction = block.body.transactions().index(0);

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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
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
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(1))), 0),
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
    Ok(())
}

#[tokio::test]
async fn get_block_transaction_count() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let transaction_count = 5;
    let block = get_test_block(transaction_count);
    storage_writer
        .begin_rw_txn()?
        .append_header(block.header.block_number, &block.header)?
        .append_body(block.header.block_number, block.body)?
        .commit()?;

    // Get block by hash.
    let res = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash))],
        )
        .await?;
    assert_eq!(res, transaction_count);

    // Get block by number.
    let res = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(block.header.block_number))],
        )
        .await?;
    assert_eq!(res, transaction_count);

    // Ask for the latest block.
    let res = module
        .call::<_, usize>("starknet_getBlockTransactionCount", [BlockId::Tag(Tag::Latest)])
        .await?;
    assert_eq!(res, transaction_count);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn get_state_update() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let (parent_header, header, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(parent_header.block_number, &parent_header)?
        .append_state_diff(parent_header.block_number, starknet_api::StateDiff::default(), vec![])?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)?
        .commit()?;

    let expected_update = StateUpdate {
        block_hash: header.block_hash,
        new_root: header.state_root,
        old_root: parent_header.state_root,
        state_diff: diff.into(),
    };

    // Get state update by block hash.
    let res = module
        .call::<_, StateUpdate>(
            "starknet_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))],
        )
        .await?;
    assert_eq!(res, expected_update);

    // Get state update by block number.
    let res = module
        .call::<_, StateUpdate>(
            "starknet_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))],
        )
        .await?;
    assert_eq!(res, expected_update);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, StateUpdate>(
            "starknet_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(2)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn get_transaction_receipt() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(1);
    storage_writer
        .begin_rw_txn()?
        .append_header(block.header.block_number, &block.header)?
        .append_body(block.header.block_number, block.body.clone())?
        .commit()?;

    let transaction_hash = block.body.transactions().index(0).transaction_hash();
    let expected_receipt = TransactionReceiptWithStatus {
        receipt: TransactionReceipt {
            transaction_hash,
            block_hash: block.header.block_hash,
            block_number: block.header.block_number,
            output: block.body.transaction_outputs().index(0).clone().into(),
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
    assert_eq!(serde_json::to_string(&res)?, serde_json::to_string(&expected_receipt)?);

    // Ask for an invalid transaction.
    let err = module
        .call::<_, TransactionReceiptWithStatus>(
            "starknet_getTransactionReceipt",
            [TransactionHash(StarkHash::from_u64(1))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::TransactionHashNotFound as i32,
        JsonRpcError::TransactionHashNotFound.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn get_class() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let (parent_header, header, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(parent_header.block_number, &parent_header)?
        .append_state_diff(parent_header.block_number, starknet_api::StateDiff::default(), vec![])?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone(), deployed_contract_class_definitions)?
        .commit()?;

    let declared_contract = diff.declared_contracts().index(1);
    let class_hash = declared_contract.class_hash;
    let expected_contract_class = declared_contract.contract_class.clone().try_into()?;

    // Get class by block hash.
    let res = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), class_hash),
        )
        .await?;
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), class_hash),
        )
        .await?;
    assert_eq!(res, expected_contract_class);

    // Ask for an invalid class hash.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ClassHash::new(shash!("0x7")),
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
                class_hash,
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
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
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(2))), class_hash),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn get_class_at() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let (parent_header, header, diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(parent_header.block_number, &parent_header)?
        .append_state_diff(parent_header.block_number, starknet_api::StateDiff::default(), vec![])?
        .append_header(header.block_number, &header)?
        .append_state_diff(
            header.block_number,
            diff.clone(),
            deployed_contract_class_definitions.clone(),
        )?
        .commit()?;

    let deployed_contracts = diff.deployed_contracts();
    let address = deployed_contracts.index(1).address;
    let hash = deployed_contracts.index(1).class_hash;
    let expected_contract_class = deployed_contract_class_definitions
        .iter()
        .find(|c| c.class_hash == hash)
        .unwrap()
        .contract_class
        .clone()
        .try_into()?;

    // Get class by block hash.
    let res = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), address),
        )
        .await?;
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), address),
        )
        .await?;
    assert_eq!(res, expected_contract_class);

    // Ask for an invalid contract.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress::try_from(shash!("0x12")).unwrap(),
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
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(parent_header.block_number)), address),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash::new(shash!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                address,
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
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber::new(2))), address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn chain_id() -> Result<(), anyhow::Error> {
    let (storage_reader, _) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let res = module.call::<_, String>("starknet_chainId", EmptyParams::new()).await?;
    // The result should be equal to the result of the following python code
    // hex(int.from_bytes(b'SN_GOERLI', byteorder="big", signed=False))
    // taken from starknet documentation:
    // https://docs.starknet.io/documentation/develop/Blocks/transactions/#chain-id.
    assert_eq!(res, String::from("0x534e5f474f45524c49"));
    Ok(())
}

#[tokio::test]
async fn get_6_events_chunk_size_2_with_address() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(2);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()?
        .append_header(block_number, &block.header)?
        .append_body(block_number, block.body.clone())?
        .commit()?;

    // Create the filter. The allowed keys at index 0 are 0x7 or 0x9.
    let mut filter_keys = HashSet::new();
    filter_keys.insert(EventKey(shash!("0x7")));
    filter_keys.insert(EventKey(shash!("0x6")));
    let block_id = BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number));
    let mut filter = EventFilter {
        from_block: Some(block_id),
        to_block: Some(block_id),
        continuation_token: None,
        chunk_size: 2,
        address: Some(ContractAddress::try_from(shash!("0x22"))?),
        keys: vec![filter_keys],
    };

    // Create the events emitted from contract address 0x22 that have at least one of the allowed
    // keys at index 0.
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let event1 = block.body.transaction_outputs().index(0).events().index(1);
    let event4 = block.body.transaction_outputs().index(0).events().index(4);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber::new(0);
    let tx_hash1 = TransactionHash(StarkHash::from_u64(0));
    let tx_hash3 = TransactionHash(StarkHash::from_u64(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event1.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event4.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event1.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event4.clone() },
    ];

    // Create the expected continuation token and chunks of events.
    let expected_continuation_token0 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(4),
        )))?;
    let expected_continuation_token1 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(1)),
            EventIndexInTransactionOutput(1),
        )))?;
    let expected_events0 = vec![emitted_events[0].clone(), emitted_events[1].clone()];
    let expected_events1 = vec![emitted_events[2].clone(), emitted_events[3].clone()];
    let expected_events2 = vec![emitted_events[4].clone(), emitted_events[5].clone()];

    // Get first chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await?;
    assert_eq!(res, expected_events0);
    assert_eq!(continuation_token, Some(expected_continuation_token0));

    // Get second chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await?;
    assert_eq!(res, expected_events1);
    assert_eq!(continuation_token, Some(expected_continuation_token1));

    // Get third chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter])
        .await?;
    assert_eq!(res, expected_events2);
    assert_eq!(continuation_token, None);

    Ok(())
}

#[tokio::test]
async fn get_2_events_chunk_size_2_with_address() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(2);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()?
        .append_header(block_number, &block.header)?
        .append_body(block_number, block.body.clone())?
        .commit()?;

    // Create the filter. The allowed key at index 1 is 0x6.
    let mut filter_keys = HashSet::new();
    filter_keys.insert(EventKey(shash!("0x6")));
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: 2,
        address: Some(ContractAddress::try_from(shash!("0x22"))?),
        keys: vec![HashSet::new(), filter_keys],
    };

    // Create the events emitted from contract address 0x2 that have at least one of the allowed
    // keys at index 0.
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber::new(0);
    let tx_hash1 = TransactionHash(StarkHash::from_u64(0));
    let tx_hash3 = TransactionHash(StarkHash::from_u64(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
    ];

    // Create the expected chunk of events.
    let expected_events0 = vec![emitted_events[0].clone(), emitted_events[1].clone()];

    // Get the only chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await?;
    assert_eq!(res, expected_events0);
    assert_eq!(continuation_token, None);

    Ok(())
}

#[tokio::test]
async fn get_4_events_chunk_size_3_with_address() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(2);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()?
        .append_header(block_number, &block.header)?
        .append_body(block_number, block.body.clone())?
        .commit()?;

    // Create the filter. The allowed keys at index 0 are 0x7 or 0x9.
    let mut filter_keys = HashSet::new();
    filter_keys.insert(EventKey(shash!("0x7")));
    filter_keys.insert(EventKey(shash!("0x9")));
    let block_id = BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number));
    let mut filter = EventFilter {
        from_block: Some(block_id),
        to_block: None,
        continuation_token: None,
        chunk_size: 3,
        address: Some(ContractAddress::try_from(shash!("0x22"))?),
        keys: vec![filter_keys],
    };

    // Create the events emitted from contract address 0x2 that have at least one of the allowed
    // keys at index 0.
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let event3 = block.body.transaction_outputs().index(0).events().index(3);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber::new(0);
    let tx_hash1 = TransactionHash(StarkHash::from_u64(0));
    let tx_hash3 = TransactionHash(StarkHash::from_u64(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event3.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event3.clone() },
    ];

    // Create the expected continuation token and chunks of events.
    let expected_continuation_token0 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(1)),
            EventIndexInTransactionOutput(3),
        )))?;
    let expected_events0 =
        vec![emitted_events[0].clone(), emitted_events[1].clone(), emitted_events[2].clone()];
    let expected_events1 = vec![emitted_events[3].clone()];

    // Get first chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await?;
    assert_eq!(res, expected_events0);
    assert_eq!(continuation_token, Some(expected_continuation_token0));

    // Get second chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter])
        .await?;
    assert_eq!(res, expected_events1);
    assert_eq!(continuation_token, None);

    Ok(())
}

#[tokio::test]
async fn get_6_events_chunk_size_2_without_address() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(2);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()?
        .append_header(block_number, &block.header)?
        .append_body(block_number, block.body.clone())?
        .commit()?;

    // Create the filter. The allowed keys at index 0 are 0x7 or 0x9.
    let mut filter_keys = HashSet::new();
    filter_keys.insert(EventKey(shash!("0x7")));
    filter_keys.insert(EventKey(shash!("0x9")));
    let mut filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: 2,
        address: None,
        keys: vec![filter_keys],
    };

    // Create the events emitted from contract address 0x2 that have at least one of the allowed
    // keys at index 0.
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let event2 = block.body.transaction_outputs().index(0).events().index(2);
    let event3 = block.body.transaction_outputs().index(0).events().index(3);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber::new(0);
    let tx_hash1 = TransactionHash(StarkHash::from_u64(0));
    let tx_hash3 = TransactionHash(StarkHash::from_u64(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event2.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event3.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event2.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event3.clone() },
    ];

    // Create the expected continuation token and chunks of events.
    let expected_continuation_token0 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(3),
        )))?;
    let expected_continuation_token1 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(1)),
            EventIndexInTransactionOutput(2),
        )))?;
    let expected_events0 = vec![emitted_events[0].clone(), emitted_events[1].clone()];
    let expected_events1 = vec![emitted_events[2].clone(), emitted_events[3].clone()];
    let expected_events2 = vec![emitted_events[4].clone(), emitted_events[5].clone()];

    // Get first chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await?;
    assert_eq!(res, expected_events0);
    assert_eq!(continuation_token, Some(expected_continuation_token0));

    // Get second chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await?;
    assert_eq!(res, expected_events1);
    assert_eq!(continuation_token, Some(expected_continuation_token1));

    // Get third chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter])
        .await?;
    assert_eq!(res, expected_events2);
    assert_eq!(continuation_token, None);

    Ok(())
}

#[tokio::test]
async fn get_6_events_chunk_size_4_without_address() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let chain_id = get_test_chain_id();
    let module = JsonRpcServerImpl { chain_id, storage_reader, max_events_chunk_size: 10, max_events_keys: 10 }.into_rpc();

    let block = get_test_block(2);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()?
        .append_header(block_number, &block.header)?
        .append_body(block_number, block.body.clone())?
        .commit()?;

    // Create the filter. The allowed keys at index 0 are 0x7 or 0x9.
    let mut filter_keys = HashSet::new();
    filter_keys.insert(EventKey(shash!("0x7")));
    filter_keys.insert(EventKey(shash!("0x9")));
    let mut filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: 4,
        address: None,
        keys: vec![filter_keys],
    };

    // Create the events emitted from contract address 0x2 that have at least one of the allowed
    // keys at index 0.
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let event2 = block.body.transaction_outputs().index(0).events().index(2);
    let event3 = block.body.transaction_outputs().index(0).events().index(3);
    let block_hash = block.header.block_hash;
    let block_number = BlockNumber::new(0);
    let tx_hash1 = TransactionHash(StarkHash::from_u64(0));
    let tx_hash3 = TransactionHash(StarkHash::from_u64(1));
    let emitted_events = vec![
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event2.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash1, event: event3.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event0.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event2.clone() },
        Event { block_hash, block_number, transaction_hash: tx_hash3, event: event3.clone() },
    ];

    // Create the expected continuation token and chunks of events.
    let expected_continuation_token0 =
        ContinuationToken::new(ContinuationTokenAsStruct(EventIndex(
            TransactionIndex(block_number, TransactionOffsetInBlock(1)),
            EventIndexInTransactionOutput(2),
        )))?;
    let expected_events0 = vec![
        emitted_events[0].clone(),
        emitted_events[1].clone(),
        emitted_events[2].clone(),
        emitted_events[3].clone(),
    ];
    let expected_events1 = vec![emitted_events[4].clone(), emitted_events[5].clone()];

    // Get first chunk of filtered events.
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter.clone()])
        .await?;
    assert_eq!(res, expected_events0);
    assert_eq!(continuation_token, Some(expected_continuation_token0));

    // Get second chunk of filtered events.
    filter.continuation_token = continuation_token;
    let (res, continuation_token) = module
        .call::<_, (Vec<Event>, Option<ContinuationToken>)>("starknet_getEvents", [filter])
        .await?;
    assert_eq!(res, expected_events1);
    assert_eq!(continuation_token, None);

    Ok(())
}

#[tokio::test]
async fn run_server_scneario() -> Result<(), anyhow::Error> {
    let (storage_reader, _) = get_test_storage();
    let gateway_config = get_test_gateway_config();
    let (addr, _handle) = run_server(gateway_config, storage_reader).await?;
    let client = HttpClientBuilder::default().build(format!("http://{:?}", addr))?;
    let err = client.block_number().await.unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn serialize_returns_expcted_json() -> Result<(), anyhow::Error> {
    // TODO(anatg): Use the papyrus_node/main.rs, when it has configuration for running different
    // components, for openning the storage and running the server.
    let (storage_reader, mut storage_writer) = get_test_storage();
    let block0 = get_test_block(0);
    let block1 = get_alpha4_starknet_block();
    let (_, _, state_diff, deployed_contract_class_definitions) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(block0.header.block_number, &block0.header)?
        .append_body(block0.header.block_number, block0.body)?
        .append_state_diff(block0.header.block_number, StateDiff::default(), vec![])?
        .append_header(block1.header.block_number, &block1.header)?
        .append_body(block1.header.block_number, block1.body)?
        .append_state_diff(
            block1.header.block_number,
            state_diff,
            deployed_contract_class_definitions,
        )?
        .commit()?;

    let gateway_config = get_test_gateway_config();
    let (server_address, _handle) = run_server(gateway_config, storage_reader).await?;

    serde_state(server_address).await?;
    serde_block(server_address).await?;
    serde_transaction(server_address).await?;
    Ok(())
}

async fn serde_state(server_address: SocketAddr) -> Result<(), anyhow::Error> {
    let res =
        send_request(server_address, "starknet_getStateUpdate", r#"{"block_number": 1}"#).await?;
    assert_eq!(res, read_json_file("state_update.json")?);

    let res = send_request(
        server_address,
        "starknet_getClassAt",
        r#"{"block_number": 1}, "0x543e54f26ae33686f57da2ceebed98b340c3a78e9390931bd84fb711d5caabc""#,
    )
    .await?;
    assert_eq!(res, read_json_file("contract_class.json")?);

    Ok(())
}

async fn serde_block(server_address: SocketAddr) -> Result<(), anyhow::Error> {
    let res =
        send_request(server_address, "starknet_getBlockWithTxs", r#"{"block_number": 1}"#).await?;
    assert_eq!(res, read_json_file("block_with_transactions.json")?);

    let res = send_request(
        server_address,
        "starknet_getBlockWithTxHashes",
        r#"{"block_hash": "0x75e00250d4343326f322e370df4c9c73c7be105ad9f532eeb97891a34d9e4a5"}"#,
    )
    .await?;
    assert_eq!(res, read_json_file("block_with_transaction_hashes.json")?);

    let res =
        send_request(server_address, "starknet_getBlockTransactionCount", r#"{"block_number": 1}"#)
            .await?;
    let expeced: serde_json::Value =
        serde_json::from_str(r#"{"jsonrpc":"2.0","result":4,"id":"1"}"#)?;
    assert_eq!(res, expeced);

    Ok(())
}

async fn serde_transaction(server_address: SocketAddr) -> Result<(), anyhow::Error> {
    let res = send_request(
        server_address,
        "starknet_getTransactionByBlockIdAndIndex",
        r#"{"block_number": 1}, 0"#,
    )
    .await?;
    assert_eq!(res, read_json_file("deploy_transaction.json")?);

    let res = send_request(
        server_address,
        "starknet_getTransactionByHash",
        r#""0x4dd12d3b82c3d0b216503c6abf63f1ccad222461582eac82057d46c327331d2""#,
    )
    .await?;
    assert_eq!(res, read_json_file("deploy_transaction.json")?);

    let res = send_request(
        server_address,
        "starknet_getTransactionReceipt",
        r#""0x6525d9aa309e5c80abbdafcc434d53202e06866597cd6dbbc91e5894fad7155""#,
    )
    .await?;
    assert_eq!(res, read_json_file("invoke_transaction_receipt.json")?);

    Ok(())
}
