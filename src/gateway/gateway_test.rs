use jsonrpsee::core::Error;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::types::EmptyParams;

use crate::starknet::{
    shash, BlockBody, BlockHash, BlockHeader, CallData, ClassHash, DeployTransaction,
    DeployedContract, Fee, StarkHash, StateDiffForward, StorageDiff, StorageEntry, Transaction,
    TransactionVersion,
};
use crate::storage::components::{
    storage_test_utils, BodyStorageWriter, HeaderStorageWriter, StateStorageWriter,
};

use super::api::*;
use super::*;

#[tokio::test]
async fn test_block_number() -> Result<(), anyhow::Error> {
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
        .begin_rw_txn()?
        .append_header(BlockNumber(0), &BlockHeader::default())?
        .commit()?;
    let block_number = module
        .call::<_, BlockNumber>("starknet_blockNumber", EmptyParams::new())
        .await?;
    assert_eq!(block_number, BlockNumber(0));
    Ok(())
}

#[tokio::test]
async fn test_get_block_by_number_w_transaction_hashes() -> Result<(), anyhow::Error> {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let transaction_hash = TransactionHash(StarkHash::from_u64(0));
    let transaction = Transaction::Deploy(DeployTransaction {
        transaction_hash,
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x2")),
        constructor_calldata: CallData(vec![shash!("0x3")]),
    });
    let body = BlockBody {
        transactions: vec![transaction.clone()],
    };
    storage_writer
        .begin_rw_txn()?
        .append_header(BlockNumber(0), &BlockHeader::default())?
        .append_body(BlockNumber(0), &body)?
        .commit()?;

    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashesByNumber",
            [BlockNumberOrTag::Number(BlockNumber(0))],
        )
        .await?;
    let block_header = &BlockHeader::default();
    let expected_block = Block {
        block_hash: block_header.block_hash,
        parent_hash: block_header.parent_hash,
        block_number: BlockNumber(0),
        status: block_header.status.into(),
        sequencer: block_header.sequencer,
        new_root: block_header.state_root,
        accepted_time: block_header.timestamp,
        transactions: Transactions::Hashes(vec![transaction_hash]),
    };
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashesByNumber",
            [BlockNumberOrTag::Tag(Tag::Latest)],
        )
        .await?;
    assert_eq!(block, expected_block);

    // Ask for an invalid block.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashesByNumber",
            [BlockNumberOrTag::Number(BlockNumber(1))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockNumber as i32,
        JsonRpcError::InvalidBlockNumber.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn test_get_block_by_number_w_full_transactions() -> Result<(), anyhow::Error> {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let transaction_hash = TransactionHash(StarkHash::from_u64(0));
    let transaction = Transaction::Deploy(DeployTransaction {
        transaction_hash,
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x2")),
        constructor_calldata: CallData(vec![shash!("0x3")]),
    });
    let body = BlockBody {
        transactions: vec![transaction.clone()],
    };
    storage_writer
        .begin_rw_txn()?
        .append_header(BlockNumber(0), &BlockHeader::default())?
        .append_body(BlockNumber(0), &body)?
        .commit()?;

    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxsByNumber",
            [BlockNumberOrTag::Number(BlockNumber(0))],
        )
        .await?;
    let block_header = &BlockHeader::default();
    let expected_block = Block {
        block_hash: block_header.block_hash,
        parent_hash: block_header.parent_hash,
        block_number: BlockNumber(0),
        status: block_header.status.into(),
        sequencer: block_header.sequencer,
        new_root: block_header.state_root,
        accepted_time: block_header.timestamp,
        transactions: Transactions::Full(vec![transaction.clone()]),
    };
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxsByNumber",
            [BlockNumberOrTag::Tag(Tag::Latest)],
        )
        .await?;
    assert_eq!(block, expected_block);

    // Ask for an invalid block.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxsByNumber",
            [BlockNumberOrTag::Number(BlockNumber(1))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockNumber as i32,
        JsonRpcError::InvalidBlockNumber.to_string(),
        None::<()>,
    ));
    Ok(())
}

#[tokio::test]
async fn test_get_block_by_hash_w_transaction_hashes() -> Result<(), anyhow::Error> {
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
    let transaction_hash = TransactionHash(StarkHash::from_u64(0));
    let transaction = Transaction::Deploy(DeployTransaction {
        transaction_hash,
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x2")),
        constructor_calldata: CallData(vec![shash!("0x3")]),
    });
    let body = BlockBody {
        transactions: vec![transaction.clone()],
    };
    storage_writer
        .begin_rw_txn()?
        .append_header(header.number, &header)?
        .append_body(header.number, &body)?
        .commit()?;

    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashesByHash",
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
        transactions: Transactions::Hashes(vec![transaction_hash]),
    };
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashesByHash",
            [BlockHashOrTag::Tag(Tag::Latest)],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for an invalid block.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxHashesByHash",
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
    Ok(())
}

#[tokio::test]
async fn test_get_block_by_hash_w_full_transactions() -> Result<(), anyhow::Error> {
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
    let transaction_hash = TransactionHash(StarkHash::from_u64(0));
    let transaction = Transaction::Deploy(DeployTransaction {
        transaction_hash,
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x2")),
        constructor_calldata: CallData(vec![shash!("0x3")]),
    });
    let body = BlockBody {
        transactions: vec![transaction.clone()],
    };
    storage_writer
        .begin_rw_txn()?
        .append_header(header.number, &header)?
        .append_body(header.number, &body)?
        .commit()?;

    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxsByHash",
            [BlockHashOrTag::Hash(block_hash)],
        )
        .await?;
    let block_header = &BlockHeader::default();
    let expected_block = Block {
        block_hash,
        parent_hash: block_header.parent_hash,
        block_number: block_header.number,
        status: block_header.status.into(),
        sequencer: block_header.sequencer,
        new_root: block_header.state_root,
        accepted_time: block_header.timestamp,
        transactions: Transactions::Full(vec![transaction.clone()]),
    };
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>(
            "starknet_getBlockWithTxsByHash",
            [BlockHashOrTag::Tag(Tag::Latest)],
        )
        .await?;
    assert_eq!(block, expected_block);

    // Ask for an invalid block.
    let err = module
        .call::<_, Block>(
            "starknet_getBlockWithTxsByHash",
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
    Ok(())
}

#[tokio::test]
async fn test_get_storage_at() -> Result<(), anyhow::Error> {
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
        .begin_rw_txn()?
        .append_header(header.number, &header)?
        .append_state_diff(BlockNumber(0), &diff)?
        .commit()?;

    let res = module
        .call::<_, StarkFelt>(
            "starknet_getStorageAt",
            (address, key.clone(), BlockHashOrTag::Hash(block_hash)),
        )
        .await?;
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
    Ok(())
}

#[tokio::test]
async fn test_get_transaction_by_hash() -> Result<(), anyhow::Error> {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let transaction_hash = TransactionHash(StarkHash::from_u64(0));
    let transaction = Transaction::Deploy(DeployTransaction {
        transaction_hash,
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x2")),
        constructor_calldata: CallData(vec![shash!("0x3")]),
    });
    let body = BlockBody {
        transactions: vec![transaction.clone()],
    };
    storage_writer
        .begin_rw_txn()?
        .append_body(BlockNumber(0), &body)?
        .commit()?;

    let res = module
        .call::<_, Transaction>("starknet_getTransactionByHash", [transaction_hash])
        .await
        .unwrap();
    assert_eq!(res, transaction.clone());

    // Ask for an invalid transaction.
    let err = module
        .call::<_, Transaction>(
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
async fn test_get_transaction_by_block_hash_and_index() -> Result<(), anyhow::Error> {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let transaction_hash = TransactionHash(StarkHash::from_u64(0));
    let transaction = Transaction::Deploy(DeployTransaction {
        transaction_hash,
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x2")),
        constructor_calldata: CallData(vec![shash!("0x3")]),
    });
    let block_hash = BlockHash(shash!(
        "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"
    ));
    let header = BlockHeader {
        block_hash,
        ..BlockHeader::default()
    };
    let body = BlockBody {
        transactions: vec![transaction.clone()],
    };
    storage_writer
        .begin_rw_txn()?
        .append_header(header.number, &header)?
        .append_body(header.number, &body)?
        .commit()?;

    let res = module
        .call::<_, Transaction>(
            "starknet_getTransactionByBlockHashAndIndex",
            (BlockHashOrTag::Hash(block_hash), 0),
        )
        .await
        .unwrap();
    assert_eq!(res, transaction.clone());

    // Ask for an invalid block.
    let err = module
        .call::<_, Transaction>(
            "starknet_getTransactionByBlockHashAndIndex",
            (
                BlockHashOrTag::Hash(BlockHash(shash!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                ))),
                0,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockHash as i32,
        JsonRpcError::InvalidBlockHash.to_string(),
        None::<()>,
    ));

    // Ask for an invalid transaction.
    let err = module
        .call::<_, Transaction>(
            "starknet_getTransactionByBlockHashAndIndex",
            (BlockHashOrTag::Hash(block_hash), 1),
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
async fn test_get_transaction_by_block_number_and_index() -> Result<(), anyhow::Error> {
    let storage_components = storage_test_utils::get_test_storage();
    let storage_reader = storage_components.block_storage_reader;
    let mut storage_writer = storage_components.block_storage_writer;
    let module = JsonRpcServerImpl { storage_reader }.into_rpc();

    let transaction_hash = TransactionHash(StarkHash::from_u64(0));
    let transaction = Transaction::Deploy(DeployTransaction {
        transaction_hash,
        max_fee: Fee(100),
        version: TransactionVersion(shash!("0x1")),
        contract_address: ContractAddress(shash!("0x2")),
        constructor_calldata: CallData(vec![shash!("0x3")]),
    });
    let block_number = BlockNumber(0);
    let header = BlockHeader {
        number: block_number,
        ..BlockHeader::default()
    };
    let body = BlockBody {
        transactions: vec![transaction.clone()],
    };
    storage_writer
        .begin_rw_txn()?
        .append_header(block_number, &header)?
        .append_body(block_number, &body)?
        .commit()?;

    let res = module
        .call::<_, Transaction>(
            "starknet_getTransactionByBlockNumberAndIndex",
            (BlockNumberOrTag::Number(block_number), 0),
        )
        .await
        .unwrap();
    assert_eq!(res, transaction.clone());

    // Ask for an invalid block.
    let err = module
        .call::<_, Transaction>(
            "starknet_getTransactionByBlockNumberAndIndex",
            (BlockNumberOrTag::Number(BlockNumber(1)), 0),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(CallError::Custom(err)) if err == ErrorObject::owned(
        JsonRpcError::InvalidBlockNumber as i32,
        JsonRpcError::InvalidBlockNumber.to_string(),
        None::<()>,
    ));

    // Ask for an invalid transaction.
    let err = module
        .call::<_, Transaction>(
            "starknet_getTransactionByBlockNumberAndIndex",
            (BlockNumberOrTag::Number(block_number), 1),
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
async fn test_run_server() -> Result<(), anyhow::Error> {
    let storage_reader = storage_test_utils::get_test_storage().block_storage_reader;
    let (addr, _handle) = run_server(
        GatewayConfig {
            server_ip: String::from("127.0.0.1:0"),
        },
        storage_reader,
    )
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
