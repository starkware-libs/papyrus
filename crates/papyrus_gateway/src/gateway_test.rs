use assert_matches::assert_matches;
use jsonrpsee::core::Error;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::http_server::types::error::CallError;
use jsonrpsee::types::error::ErrorObject;
use jsonrpsee::types::EmptyParams;
use papyrus_storage::test_utils::{get_test_block, get_test_storage};
use papyrus_storage::{
    BodyStorageWriter, HeaderStorageWriter, StateStorageWriter, StatusStorageWriter,
};
use starknet_api::{
    shash, BlockHash, BlockHeader, BlockNumber, ClassHash, ContractAddress, ContractClass,
    DeclareTransactionOutput, DeployedContract, GlobalRoot, Nonce, StarkFelt, StarkHash, StateDiff,
    StorageDiff, StorageEntry, StorageKey, TransactionHash, TransactionOutput, TransactionReceipt,
};

use super::api::{
    BlockHashAndNumber, BlockHashOrNumber, BlockId, JsonRpcClient, JsonRpcError, JsonRpcServer, Tag,
};
use super::objects::{
    from_starknet_storage_diffs, Block, GateWayStateDiff, StateUpdate, TransactionWithType,
    Transactions,
};
use super::{run_server, GatewayConfig, JsonRpcServerImpl};

fn get_test_state_diff() -> (BlockHeader, BlockHeader, StateDiff) {
    let parent_hash =
        BlockHash(shash!("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"));
    let state_root = GlobalRoot(shash!("0x12"));
    let parent_header = BlockHeader {
        block_number: BlockNumber(0),
        block_hash: parent_hash,
        state_root,
        ..BlockHeader::default()
    };

    let block_hash =
        BlockHash(shash!("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5493"));
    let header = BlockHeader {
        block_number: BlockNumber(1),
        block_hash,
        parent_hash,
        ..BlockHeader::default()
    };

    let address0 = ContractAddress(shash!("0x11"));
    let hash0 = ClassHash(shash!("0x4"));
    let address1 = ContractAddress(shash!("0x21"));
    let hash1 = ClassHash(shash!("0x5"));
    let class0 = ContractClass::default();
    let class1 = ContractClass::default();
    let key0 = StorageKey(shash!("0x1001"));
    let value0 = shash!("0x200");
    let key1 = StorageKey(shash!("0x1002"));
    let value1 = shash!("0x201");
    let diff = StateDiff::new(
        vec![
            DeployedContract { address: address0, class_hash: hash0 },
            DeployedContract { address: address1, class_hash: hash1 },
        ],
        vec![
            StorageDiff {
                address: address0,
                diff: vec![
                    StorageEntry { key: key0.clone(), value: value0 },
                    StorageEntry { key: key1, value: value1 },
                ],
            },
            StorageDiff {
                address: address1,
                diff: vec![StorageEntry { key: key0, value: value0 }],
            },
        ],
        vec![(hash0, class0), (hash1, class1)],
        vec![(address0, Nonce(StarkHash::from_u64(1))), (address1, Nonce(StarkHash::from_u64(1)))],
    )
    .unwrap();

    (parent_header, header, diff)
}

#[tokio::test]
async fn test_block_number() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
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
        .begin_rw_txn()?
        .append_header(BlockNumber(0), &BlockHeader::default())?
        .commit()?;
    let block_number =
        module.call::<_, BlockNumber>("starknet_blockNumber", EmptyParams::new()).await?;
    assert_eq!(block_number, BlockNumber(0));
    Ok(())
}

#[tokio::test]
async fn test_block_hash_and_number() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

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
    let (_, header, _) = get_test_block(1);
    storage_writer.begin_rw_txn()?.append_header(header.block_number, &header)?.commit()?;
    let block_hash_and_number = module
        .call::<_, BlockHashAndNumber>("starknet_blockHashAndNumber", EmptyParams::new())
        .await?;
    assert_eq!(
        block_hash_and_number,
        BlockHashAndNumber { block_hash: header.block_hash, block_number: header.block_number }
    );
    Ok(())
}

#[tokio::test]
async fn test_get_block_w_transaction_hashes() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (block_status, header, body) = get_test_block(1);
    storage_writer
        .begin_rw_txn()?
        .append_block_status(header.block_number, &block_status)?
        .append_header(header.block_number, &header)?
        .append_body(header.block_number, &body)?
        .commit()?;

    let expected_transaction = body.transactions.get(0).unwrap();
    let expected_block = Block {
        status: block_status,
        header: header.into(),
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
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
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn test_get_block_w_full_transactions() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (block_status, header, body) = get_test_block(1);
    storage_writer
        .begin_rw_txn()?
        .append_block_status(header.block_number, &block_status)?
        .append_header(header.block_number, &header)?
        .append_body(header.block_number, &body)?
        .commit()?;

    let expected_transaction = body.transactions.get(0).unwrap();
    let expected_block = Block {
        status: block_status,
        header: header.into(),
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
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
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn test_get_storage_at() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (header, _, diff) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone())?
        .commit()?;

    let (_, storage_diffs, _, _) = diff.destruct();

    let address = storage_diffs.get(0).unwrap().address;
    let key = storage_diffs.get(0).unwrap().diff.get(0).unwrap().key.clone();
    let expected_value = storage_diffs.get(0).unwrap().diff.get(0).unwrap().value;

    // Get storage by block hash.
    let res = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                address,
                key.clone(),
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)),
            ),
        )
        .await?;
    assert_eq!(res, expected_value);

    // Get storage by block number.
    let res = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                address,
                key.clone(),
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
            ),
        )
        .await?;
    assert_eq!(res, expected_value);

    // Ask for an invalid contract.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                ContractAddress(shash!("0x12")),
                key.clone(),
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
                key.clone(),
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (
                address,
                key.clone(),
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn test_get_class_hash_at() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (header, _, diff) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone())?
        .commit()?;

    let (deployed_contracts, _, _, _) = diff.destruct();

    let address = deployed_contracts.get(0).unwrap().address;
    let expected_class_hash = deployed_contracts.get(0).unwrap().class_hash;

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
                ContractAddress(shash!("0x12")),
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
                address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, ClassHash>(
            "starknet_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))), address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn test_get_nonce() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    // TODO(anatg): Write the nonces to the storage once it's supported.
    let (header, _, diff) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone())?
        .commit()?;

    let (deployed_contracts, _, _, _) = diff.destruct();
    let address = deployed_contracts.get(0).unwrap().address;
    let expected_nonce = Nonce::default();

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

    // TODO(anatg): Ask for an invalid contract.

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, Nonce>(
            "starknet_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))), address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_hash() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (_, _, body) = get_test_block(1);
    storage_writer.begin_rw_txn()?.append_body(BlockNumber(0), &body)?.commit()?;

    let expected_transaction = body.transactions.get(0).unwrap();
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
        JsonRpcError::InvalidTransactionHash as i32,
        JsonRpcError::InvalidTransactionHash.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_block_id_and_index() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (_, header, body) = get_test_block(1);
    storage_writer
        .begin_rw_txn()?
        .append_header(header.block_number, &header)?
        .append_body(header.block_number, &body)?
        .commit()?;

    let expected_transaction = body.transactions.get(0).unwrap();

    // Get transaction by block hash.
    let res = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), 0),
        )
        .await
        .unwrap();
    assert_eq!(res, TransactionWithType::from(expected_transaction.clone()));

    // Get transaction by block number.
    let res = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), 0),
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
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
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
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    // Ask for an invalid transaction index.
    let err = module
        .call::<_, TransactionWithType>(
            "starknet_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), 1),
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
async fn test_get_block_transaction_count() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let transaction_count = 5;
    let (_, header, body) = get_test_block(transaction_count);
    storage_writer
        .begin_rw_txn()?
        .append_header(header.block_number, &header)?
        .append_body(header.block_number, &body)?
        .commit()?;

    // Get block by hash.
    let res = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))],
        )
        .await?;
    assert_eq!(res, transaction_count);

    // Get block by number.
    let res = module
        .call::<_, usize>(
            "starknet_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))],
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
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
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
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn test_get_state_update() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (parent_header, header, diff) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(parent_header.block_number, &parent_header)?
        .append_state_diff(parent_header.block_number, StateDiff::default())?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone())?
        .commit()?;

    let (deployed_contracts, storage_diffs, _, _) = diff.destruct();

    let expected_update = StateUpdate {
        block_hash: header.block_hash,
        new_root: header.state_root,
        old_root: parent_header.state_root,
        state_diff: GateWayStateDiff {
            storage_diffs: from_starknet_storage_diffs(storage_diffs),
            declared_classes: vec![],
            deployed_contracts,
            nonces: vec![],
        },
    };
    assert_eq!(expected_update.state_diff.storage_diffs.len(), 3);

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
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
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
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn test_get_transaction_receipt() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (_, _, body) = get_test_block(1);
    let block_number = BlockNumber(0);
    storage_writer.begin_rw_txn()?.append_body(block_number, &body)?.commit()?;
    // TODO(anatg): Write a transaction receipt to the storage.

    let transaction_hash = body.transactions.get(0).unwrap().transaction_hash();
    let expected_receipt = TransactionReceipt {
        transaction_hash,
        block_hash: BlockHash::default(),
        block_number,
        output: TransactionOutput::Declare(DeclareTransactionOutput::default()),
    };
    let res = module
        .call::<_, TransactionReceipt>("starknet_getTransactionReceipt", [transaction_hash])
        .await
        .unwrap();
    assert_eq!(res, expected_receipt.clone());

    // Ask for an invalid transaction.
    let err = module
        .call::<_, TransactionReceipt>(
            "starknet_getTransactionReceipt",
            [TransactionHash(StarkHash::from_u64(1))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidTransactionHash as i32,
        JsonRpcError::InvalidTransactionHash.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn test_get_class() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (parent_header, header, diff) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(parent_header.block_number, &parent_header)?
        .append_state_diff(parent_header.block_number, StateDiff::default())?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone())?
        .commit()?;

    let (_, _, declared_classes, _) = diff.destruct();

    let class_hash = declared_classes.get(0).unwrap().0;
    let expected_contract_class = declared_classes.get(0).unwrap().1.clone();

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
                ClassHash(shash!("0x6")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidContractClassHash as i32,
        JsonRpcError::InvalidContractClassHash.to_string(),
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
        JsonRpcError::InvalidContractClassHash as i32,
        JsonRpcError::InvalidContractClassHash.to_string(),
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
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2))), class_hash),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn test_get_class_at() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let (parent_header, header, diff) = get_test_state_diff();
    storage_writer
        .begin_rw_txn()?
        .append_header(parent_header.block_number, &parent_header)?
        .append_state_diff(parent_header.block_number, StateDiff::default())?
        .append_header(header.block_number, &header)?
        .append_state_diff(header.block_number, diff.clone())?
        .commit()?;

    let (deployed_contracts, _, declared_classes, _) = diff.destruct();
    let address = deployed_contracts.get(0).unwrap().address;
    let expected_contract_class = declared_classes.get(0).unwrap().1.clone();

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
                ContractAddress(shash!("0x12")),
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
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(shash!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, ContractClass>(
            "starknet_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2))), address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockId as i32,
        JsonRpcError::InvalidBlockId.to_string(),
        None::<()>,
    ));

    Ok(())
}

#[tokio::test]
async fn test_run_server() -> Result<(), anyhow::Error> {
    let (storage_reader, _) = get_test_storage();
    let (addr, _handle) =
        run_server(GatewayConfig { server_ip: String::from("127.0.0.1:0") }, storage_reader)
            .await?;
    let client = HttpClientBuilder::default().build(format!("http://{:?}", addr))?;
    let err = client.block_number().await.unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    ));
    Ok(())
}
