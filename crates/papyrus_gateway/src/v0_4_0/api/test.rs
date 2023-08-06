use std::collections::HashSet;
use std::net::SocketAddr;
use std::ops::Index;

use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::{indexmap, IndexMap};
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::core::Error;
use jsonrpsee::types::ErrorObjectOwned;
use jsonschema::JSONSchema;
use papyrus_common::BlockHashAndNumber;
use papyrus_execution::execution_utils::selector_from_name;
use papyrus_storage::base_layer::BaseLayerStorageWriter;
use papyrus_storage::body::events::EventIndex;
use papyrus_storage::body::{BodyStorageWriter, TransactionIndex};
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockBody, BlockHash, BlockHeader, BlockNumber, BlockStatus};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::{
    ContractClass as SN_API_DeprecatedContractClass, ContractClassAbiEntry, FunctionAbiEntry,
    FunctionAbiEntryType, FunctionAbiEntryWithType, FunctionStateMutability,
};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::StateDiff;
use starknet_api::transaction::{
    Calldata, EventIndexInTransactionOutput, EventKey, TransactionExecutionStatus, TransactionHash,
    TransactionOffsetInBlock,
};
use starknet_api::{calldata, patricia_key, stark_felt};
use test_utils::{
    get_rng, get_test_block, get_test_body, get_test_state_diff, read_json_file, send_request,
    GetTestInstance,
};

use super::super::api::EventsChunk;
use super::super::block::Block;
use super::super::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use super::super::state::{ContractClass, StateUpdate, ThinStateDiff};
use super::super::transaction::{
    Event, TransactionFinalityStatus, TransactionOutput, TransactionReceipt,
    TransactionReceiptWithStatus, TransactionWithHash, Transactions,
};
use super::api_impl::JsonRpcServerV0_4Impl;
use crate::api::{BlockHashOrNumber, BlockId, ContinuationToken, EventFilter, JsonRpcError, Tag};
use crate::test_utils::{
    get_starknet_spec_api_schema_for_components, get_test_gateway_config, get_test_highest_block,
    get_test_rpc_server_and_storage_writer, validate_schema, SpecFile,
};
use crate::version_config::VERSION_0_4;
use crate::{run_server, ContinuationTokenAsStruct};

#[tokio::test]
async fn chain_id() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    let res = module.call::<_, String>("starknet_V0_4_chainId", ObjectParams::new()).await.unwrap();
    // The result should be equal to the result of the following python code
    // hex(int.from_bytes(b'SN_GOERLI', byteorder="big", signed=False))
    // taken from starknet documentation:
    // https://docs.starknet.io/documentation/develop/Blocks/transactions/#chain-id.
    assert_eq!(res, String::from("0x534e5f474f45524c49"));
}

#[tokio::test]
async fn block_hash_and_number() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    // No blocks yet.
    let err = module
        .call::<_, BlockHashAndNumber>("starknet_V0_4_blockHashAndNumber", ObjectParams::new())
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    ));

    // Add a block and check again.
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .commit()
        .unwrap();
    let block_hash_and_number = module
        .call::<_, BlockHashAndNumber>("starknet_V0_4_blockHashAndNumber", ObjectParams::new())
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
async fn block_number() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    // No blocks yet.
    let err = module
        .call::<_, BlockNumber>("starknet_V0_4_blockNumber", ObjectParams::new())
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
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
    let block_number = module
        .call::<_, BlockNumber>("starknet_V0_4_blockNumber", ObjectParams::new())
        .await
        .unwrap();
    assert_eq!(block_number, BlockNumber(0));
}

#[tokio::test]
async fn syncing() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let res = module.call::<_, bool>("starknet_V0_4_syncing", ObjectParams::new()).await.unwrap();
    assert_eq!(res, false);
}

#[tokio::test]
async fn get_block_transaction_count() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let transaction_count = 5;
    let block = get_test_block(transaction_count, None, None, None);
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
            "starknet_V0_4_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(res, transaction_count);

    // Get block by number.
    let res = module
        .call::<_, usize>(
            "starknet_V0_4_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(res, transaction_count);

    // Ask for the latest block.
    let res = module
        .call::<_, usize>("starknet_V0_4_getBlockTransactionCount", [BlockId::Tag(Tag::Latest)])
        .await
        .unwrap();
    assert_eq!(res, transaction_count);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, usize>(
            "starknet_V0_4_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, usize>(
            "starknet_V0_4_getBlockTransactionCount",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_block_w_full_transactions() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = TransactionWithHash {
        transaction: block.body.transactions[0].clone().into(),
        transaction_hash: block.body.transaction_hashes[0],
    };
    let expected_block = Block {
        status: BlockStatus::AcceptedOnL2,
        header: block.header.into(),
        transactions: Transactions::Full(vec![expected_transaction]),
    };

    // Get block by hash.
    let block = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Get block by number.
    let block = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>("starknet_V0_4_getBlockWithTxs", [BlockId::Tag(Tag::Latest)])
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for a block that was accepted on L1.
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_block_marker(&expected_block.header.block_number.next())
        .unwrap()
        .commit()
        .unwrap();
    let block = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block.status, BlockStatus::AcceptedOnL1);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxs",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_block_w_transaction_hashes() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_block = Block {
        status: BlockStatus::AcceptedOnL2,
        header: block.header.into(),
        transactions: Transactions::Hashes(vec![block.body.transaction_hashes[0]]),
    };

    // Get block by hash.
    let block = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Get block by number.
    let block = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module
        .call::<_, Block>("starknet_V0_4_getBlockWithTxHashes", [BlockId::Tag(Tag::Latest)])
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for a block that was accepted on L1.
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_block_marker(&expected_block.header.block_number.next())
        .unwrap()
        .commit()
        .unwrap();
    let block = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block.status, BlockStatus::AcceptedOnL1);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, Block>(
            "starknet_V0_4_getBlockWithTxHashes",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_class() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
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

    // Deprecated Class
    let (class_hash, contract_class) = diff.deprecated_declared_classes.get_index(0).unwrap();
    let expected_contract_class = contract_class.clone().try_into().unwrap();

    // Get class by block hash.
    let res = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Ask for an invalid class hash.
    let err = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ClassHash(stark_felt!("0x7")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ClassHashNotFound as i32,
        JsonRpcError::ClassHashNotFound.to_string(),
        None::<()>,
    ));

    // New Class
    let (class_hash, (_compiled_class_hash, contract_class)) =
        diff.declared_classes.get_index(0).unwrap();
    let expected_contract_class = contract_class.clone().into();

    // Get class by block hash.
    let res = module
        .call::<_, ContractClass>(
            "starknet_V0_4_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, ContractClass>(
            "starknet_V0_4_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Invalid Call
    // Ask for an invalid class hash in the given block.
    let err = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(parent_header.block_number)),
                *class_hash,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ClassHashNotFound as i32,
        JsonRpcError::ClassHashNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                class_hash,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClass",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2))), *class_hash),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_transaction_receipt() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let transaction_hash = block.body.transaction_hashes[0];
    let output = TransactionOutput::from(block.body.transaction_outputs.index(0).clone());
    let expected_receipt = TransactionReceiptWithStatus {
        receipt: TransactionReceipt {
            transaction_hash,
            block_hash: block.header.block_hash,
            block_number: block.header.block_number,
            output,
        },
        finality_status: TransactionFinalityStatus::AcceptedOnL2,
        execution_status: TransactionExecutionStatus::default(),
    };
    let res = module
        .call::<_, TransactionReceiptWithStatus>(
            "starknet_V0_4_getTransactionReceipt",
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

    // Ask for a transaction in a block that was accepted on L1.
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_block_marker(&block.header.block_number.next())
        .unwrap()
        .commit()
        .unwrap();
    let res = module
        .call::<_, TransactionReceiptWithStatus>(
            "starknet_V0_4_getTransactionReceipt",
            [transaction_hash],
        )
        .await
        .unwrap();
    assert_eq!(res.finality_status, TransactionFinalityStatus::AcceptedOnL1);
    assert_eq!(res.execution_status, TransactionExecutionStatus::Succeeded);

    // Ask for an invalid transaction.
    let err = module
        .call::<_, TransactionReceiptWithStatus>(
            "starknet_V0_4_getTransactionReceipt",
            [TransactionHash(StarkHash::from(1_u8))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::TransactionHashNotFound as i32,
        JsonRpcError::TransactionHashNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_class_at() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let parent_header = BlockHeader::default();
    let header = BlockHeader {
        block_hash: BlockHash(stark_felt!("0x1")),
        block_number: BlockNumber(1),
        parent_hash: parent_header.block_hash,
        ..BlockHeader::default()
    };
    let mut diff = get_test_state_diff();
    // Add a deployed contract with Cairo 1 class.
    let new_class_hash = diff.declared_classes.get_index(0).unwrap().0;
    diff.deployed_contracts.insert(ContractAddress(patricia_key!("0x2")), *new_class_hash);
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

    // Deprecated Class
    let (class_hash, contract_class) = diff.deprecated_declared_classes.get_index(0).unwrap();
    let expected_contract_class = contract_class.clone().try_into().unwrap();
    assert_eq!(diff.deployed_contracts.get_index(0).unwrap().1, class_hash);
    let address = diff.deployed_contracts.get_index(0).unwrap().0;

    // Get class by block hash.
    let res = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // New Class
    let (class_hash, (_compiled_hash, contract_class)) =
        diff.declared_classes.get_index(0).unwrap();
    let expected_contract_class = contract_class.clone().try_into().unwrap();
    assert_eq!(diff.deployed_contracts.get_index(1).unwrap().1, class_hash);
    let address = diff.deployed_contracts.get_index(1).unwrap().0;

    // Get class by block hash.
    let res = module
        .call::<_, ContractClass>(
            "starknet_V0_4_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, ContractClass>(
            "starknet_V0_4_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Invalid Call
    // Ask for an invalid contract.
    let err = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClassAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress(patricia_key!("0x12")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid contract in the given block.
    let err = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClassAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(parent_header.block_number)),
                *address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClassAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                *address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClassAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2))), *address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_class_hash_at() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
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
            "starknet_V0_4_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_class_hash);

    // Get class hash by block number.
    let res = module
        .call::<_, ClassHash>(
            "starknet_V0_4_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_class_hash);

    // Ask for an invalid contract.
    let err = module
        .call::<_, ClassHash>(
            "starknet_V0_4_getClassHashAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress(patricia_key!("0x12")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, ClassHash>(
            "starknet_V0_4_getClassHashAt",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                *address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, ClassHash>(
            "starknet_V0_4_getClassHashAt",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))), *address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_nonce() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
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
            "starknet_V0_4_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_nonce);

    // Get class hash by block number.
    let res = module
        .call::<_, Nonce>(
            "starknet_V0_4_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_nonce);

    // Ask for an invalid contract.
    let err = module
        .call::<_, Nonce>(
            "starknet_V0_4_getNonce",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
                ContractAddress(patricia_key!("0x31")),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Nonce>(
            "starknet_V0_4_getNonce",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                *address,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, Nonce>(
            "starknet_V0_4_getNonce",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))), *address),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_storage_at() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
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
            "starknet_V0_4_getStorageAt",
            (*address, *key, BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Get storage by block number.
    let res = module
        .call::<_, StarkFelt>(
            "starknet_V0_4_getStorageAt",
            (*address, *key, BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Ask for an invalid contract.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_V0_4_getStorageAt",
            (
                ContractAddress(patricia_key!("0x12")),
                key,
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)),
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_V0_4_getStorageAt",
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
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, StarkFelt>(
            "starknet_V0_4_getStorageAt",
            (*address, key, BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_transaction_by_hash() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = TransactionWithHash {
        transaction: block.body.transactions[0].clone().into(),
        transaction_hash: block.body.transaction_hashes[0],
    };
    let res = module
        .call::<_, TransactionWithHash>(
            "starknet_V0_4_getTransactionByHash",
            [block.body.transaction_hashes[0]],
        )
        .await
        .unwrap();
    assert_eq!(res, expected_transaction);

    // Ask for an invalid transaction.
    let err = module
        .call::<_, TransactionWithHash>(
            "starknet_V0_4_getTransactionByHash",
            [TransactionHash(StarkHash::from(1_u8))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::TransactionHashNotFound as i32,
        JsonRpcError::TransactionHashNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_transaction_by_block_id_and_index() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = TransactionWithHash {
        transaction: block.body.transactions[0].clone().into(),
        transaction_hash: block.body.transaction_hashes[0],
    };

    // Get transaction by block hash.
    let res = module
        .call::<_, TransactionWithHash>(
            "starknet_V0_4_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash)), 0),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_transaction);

    // Get transaction by block number.
    let res = module
        .call::<_, TransactionWithHash>(
            "starknet_V0_4_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(block.header.block_number)), 0),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_transaction);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, TransactionWithHash>(
            "starknet_V0_4_getTransactionByBlockIdAndIndex",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                    "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
                )))),
                0,
            ),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, TransactionWithHash>(
            "starknet_V0_4_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1))), 0),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid transaction index.
    let err = module
        .call::<_, TransactionWithHash>(
            "starknet_V0_4_getTransactionByBlockIdAndIndex",
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash)), 1),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::InvalidTransactionIndex as i32,
        JsonRpcError::InvalidTransactionIndex.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_state_update() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
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
        state_diff: ThinStateDiff::from(starknet_api::state::ThinStateDiff::from(diff)),
    };

    // Get state update by block hash.
    let res = module
        .call::<_, StateUpdate>(
            "starknet_V0_4_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(res, expected_update);

    // Get state update by block number.
    let res = module
        .call::<_, StateUpdate>(
            "starknet_V0_4_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(res, expected_update);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, StateUpdate>(
            "starknet_V0_4_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            ))))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Ask for an invalid block number.
    let err = module
        .call::<_, StateUpdate>(
            "starknet_V0_4_getStateUpdate",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_events_chunk_size_2_with_address() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let address = ContractAddress(patricia_key!("0x22"));
    let key0 = EventKey(stark_felt!("0x6"));
    let key1 = EventKey(stark_felt!("0x7"));
    let block = get_test_block(
        2,
        Some(5),
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
        let transaction_hash = block.body.transaction_hashes[tx_i];
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
        let res = module
            .call::<_, EventsChunk>("starknet_V0_4_getEvents", [filter.clone()])
            .await
            .unwrap();
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
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let key0 = EventKey(stark_felt!("0x6"));
    let key1 = EventKey(stark_felt!("0x7"));
    let block = get_test_block(
        2,
        Some(5),
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
        let transaction_hash = block.body.transaction_hashes[tx_i];
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
        let res = module
            .call::<_, EventsChunk>("starknet_V0_4_getEvents", [filter.clone()])
            .await
            .unwrap();
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
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: get_test_gateway_config().max_events_chunk_size + 1,
        address: None,
        keys: vec![],
    };

    let err = module.call::<_, EventsChunk>("starknet_V0_4_getEvents", [filter]).await.unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::PageSizeTooBig as i32,
        JsonRpcError::PageSizeTooBig.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_events_too_many_keys() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let keys = (0..get_test_gateway_config().max_events_keys + 1)
        .map(|i| HashSet::from([EventKey(StarkFelt::from(i as u128))]))
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

    let err = module.call::<_, EventsChunk>("starknet_V0_4_getEvents", [filter]).await.unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::TooManyKeysInFilter as i32,
        JsonRpcError::TooManyKeysInFilter.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn get_events_no_blocks() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: 2,
        address: None,
        keys: vec![],
    };

    let res = module.call::<_, EventsChunk>("starknet_V0_4_getEvents", [filter]).await.unwrap();
    assert_eq!(res, EventsChunk { events: vec![], continuation_token: None });
}

#[tokio::test]
async fn get_events_no_blocks_in_filter() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let parent_block = starknet_api::block::Block::default();
    let block = starknet_api::block::Block {
        header: BlockHeader {
            parent_hash: parent_block.header.block_hash,
            block_hash: BlockHash(stark_felt!("0x1")),
            block_number: BlockNumber(1),
            ..BlockHeader::default()
        },
        body: get_test_body(1, None, None, None),
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

    let res = module.call::<_, EventsChunk>("starknet_V0_4_getEvents", [filter]).await.unwrap();
    assert_eq!(res, EventsChunk { events: vec![], continuation_token: None });
}

#[tokio::test]
async fn get_events_invalid_ct() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
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

    let err = module.call::<_, EventsChunk>("starknet_V0_4_getEvents", [filter]).await.unwrap_err();
    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::InvalidContinuationToken as i32,
        JsonRpcError::InvalidContinuationToken.to_string(),
        None::<()>,
    ));
}

#[tokio::test]
async fn serialize_returns_valid_json() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut rng = get_rng();
    let parent_block = starknet_api::block::Block::default();
    let block = starknet_api::block::Block {
        header: BlockHeader {
            parent_hash: parent_block.header.block_hash,
            block_hash: BlockHash(stark_felt!("0x1")),
            block_number: BlockNumber(1),
            ..BlockHeader::default()
        },
        body: get_test_body(5, Some(5), None, None),
    };
    let mut state_diff = StateDiff::get_test_instance(&mut rng);
    // In the test instance both declared_classes and deprecated_declared_classes have an entry
    // with class hash 0x0, which is illegal.
    state_diff.deprecated_declared_classes = IndexMap::from([(
        ClassHash(stark_felt!("0x2")),
        starknet_api::deprecated_contract_class::ContractClass::get_test_instance(&mut rng),
    )]);
    // For checking the schema also for deprecated contract classes.
    state_diff
        .deployed_contracts
        .insert(ContractAddress(patricia_key!("0x2")), ClassHash(stark_felt!("0x2")));
    // TODO(yair): handle replaced classes.
    state_diff.replaced_classes.clear();
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
    let (server_address, _handle) =
        run_server(&gateway_config, get_test_highest_block(), storage_reader).await.unwrap();

    let schema = get_starknet_spec_api_schema_for_components(
        &[(
            SpecFile::StarknetApiOpenrpc,
            &[
                "BLOCK_WITH_TXS",
                "BLOCK_WITH_TX_HASHES",
                "STATE_UPDATE",
                "CONTRACT_CLASS",
                "DEPRECATED_CONTRACT_CLASS",
                "TXN",
                "TXN_RECEIPT",
                "EVENTS_CHUNK",
            ],
        )],
        &VERSION_0_4,
    )
    .await;
    validate_state(&state_diff, server_address, &schema).await;
    validate_block(&block.header, server_address, &schema).await;
    validate_transaction(block.body.transaction_hashes.index(0), server_address, &schema).await;
}

async fn validate_state(state_diff: &StateDiff, server_address: SocketAddr, schema: &JSONSchema) {
    let res = send_request(
        server_address,
        "starknet_getStateUpdate",
        r#"{"block_number": 1}"#,
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "State update is not valid.");

    let (address, _) = state_diff.deployed_contracts.get_index(0).unwrap();
    let res = send_request(
        server_address,
        "starknet_getClassAt",
        format!(r#"{{"block_number": 1}}, "0x{}""#, hex::encode(address.0.key().bytes())).as_str(),
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "Class is not valid.");

    // TODO(dvir): Remove this after regenesis.
    // This checks the deployed deprecated class.
    let (address, _) = state_diff.deployed_contracts.get_index(1).unwrap();
    let res = send_request(
        server_address,
        "starknet_getClassAt",
        format!(r#"{{"block_number": 1}}, "0x{}""#, hex::encode(address.0.key().bytes())).as_str(),
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "Class is not valid.");
}

async fn validate_block(header: &BlockHeader, server_address: SocketAddr, schema: &JSONSchema) {
    let res = send_request(
        server_address,
        "starknet_getBlockWithTxs",
        r#"{"block_number": 1}"#,
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "Block with transactions is not valid.");

    let res = send_request(
        server_address,
        "starknet_getBlockWithTxHashes",
        format!(r#"{{"block_hash": "0x{}"}}"#, hex::encode(header.block_hash.0.bytes())).as_str(),
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "Block with transaction hashes is not valid.");
}

async fn validate_transaction(
    tx_hash: &TransactionHash,
    server_address: SocketAddr,
    schema: &JSONSchema,
) {
    let res = send_request(
        server_address,
        "starknet_getTransactionByBlockIdAndIndex",
        r#"{"block_number": 1}, 0"#,
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "Transaction is not valid.");

    let res = send_request(
        server_address,
        "starknet_getTransactionByHash",
        format!(r#""0x{}""#, hex::encode(tx_hash.0.bytes())).as_str(),
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "Transaction is not valid.");

    let res = send_request(
        server_address,
        "starknet_getTransactionReceipt",
        format!(r#""0x{}""#, hex::encode(tx_hash.0.bytes())).as_str(),
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "Transaction receipt is not valid.");

    let res = send_request(
        server_address,
        "starknet_getEvents",
        r#"{"chunk_size": 2}"#,
        VERSION_0_4.name,
    )
    .await;
    assert!(validate_schema(schema, res), "Events are not valid.");
}

// This test checks that the deprecated contract class is returned with the correct state mutability
// field in the function abi entry. If there is no stateMutability field, the gateway should return
// an answer without this field at all, and if it is present, it should be returned as is.
#[tokio::test]
async fn get_deprecated_class_state_mutability() {
    // Without state mutability.
    let function_abi_without_state_mutability =
        FunctionAbiEntry { state_mutability: None, ..Default::default() };
    let function_abi_without_state_mutability =
        ContractClassAbiEntry::Function(FunctionAbiEntryWithType {
            entry: function_abi_without_state_mutability,
            r#type: FunctionAbiEntryType::Function,
        });
    let class_without_state_mutability = starknet_api::deprecated_contract_class::ContractClass {
        abi: Some(vec![function_abi_without_state_mutability]),
        ..Default::default()
    };

    // With state mutability.
    let function_abi_with_state_mutability = FunctionAbiEntry {
        state_mutability: Some(FunctionStateMutability::View),
        ..Default::default()
    };
    let function_abi_with_state_mutability =
        ContractClassAbiEntry::Function(FunctionAbiEntryWithType {
            entry: function_abi_with_state_mutability,
            r#type: FunctionAbiEntryType::Function,
        });
    let class_with_state_mutability = starknet_api::deprecated_contract_class::ContractClass {
        abi: Some(vec![function_abi_with_state_mutability]),
        ..Default::default()
    };

    let state_diff = StateDiff {
        deprecated_declared_classes: IndexMap::from([
            (ClassHash(stark_felt!("0x0")), class_without_state_mutability),
            (ClassHash(stark_felt!("0x1")), class_with_state_mutability),
        ]),
        ..Default::default()
    };

    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_number, state_diff, IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    // Get class without state mutability.
    let res = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)),
                ClassHash(stark_felt!("0x0")),
            ),
        )
        .await
        .unwrap();
    let res_as_value = serde_json::to_value(res).unwrap();
    let entry = res_as_value["abi"][0].as_object().unwrap();
    assert!(!entry.contains_key("stateMutability"));

    // Get class with state mutability.
    let res = module
        .call::<_, DeprecatedContractClass>(
            "starknet_V0_4_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)),
                ClassHash(stark_felt!("0x1")),
            ),
        )
        .await
        .unwrap();
    let res_as_value = serde_json::to_value(res).unwrap();
    let entry = res_as_value["abi"][0].as_object().unwrap();
    assert_eq!(entry.get("stateMutability").unwrap().as_str().unwrap(), "view");
}

#[tokio::test]
async fn execution_call() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let address = ContractAddress(patricia_key!("0x1"));
    let key = stark_felt!(1234_u16);
    let value = stark_felt!(18_u8);

    let res = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
            (
                address,
                selector_from_name("test_storage_read_write"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap();

    assert_eq!(res, vec![value]);

    // Calling a non-existent contract.
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
            (
                ContractAddress(patricia_key!("0x1234")),
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ContractNotFound as i32,
        JsonRpcError::ContractNotFound.to_string(),
        None::<()>,
    ));

    // Calling a non-existent block.
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
            (
                ContractAddress(patricia_key!("0x1234")),
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(7))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::BlockNotFound as i32,
        JsonRpcError::BlockNotFound.to_string(),
        None::<()>,
    ));

    // Calling a non-existent function (contract error).
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
            (
                address,
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == ErrorObjectOwned::owned(
        JsonRpcError::ContractError as i32,
        JsonRpcError::ContractError.to_string(),
        None::<()>,
    ));
}

fn prepare_storage_for_execution(mut storage_writer: StorageWriter) {
    let class_hash1 = ClassHash(1u128.into());
    let class1 = serde_json::from_value::<SN_API_DeprecatedContractClass>(read_json_file(
        "deprecated_class.json",
    ))
    .unwrap();
    let address1 = ContractAddress(patricia_key!("0x1"));

    let class_hash2 = ClassHash(StarkFelt::from(2u128));
    let address2 = ContractAddress(patricia_key!("0x2"));
    // The class is not used in the execution, so it can be default.
    let class2 = starknet_api::state::ContractClass::default();
    let casm = serde_json::from_value::<CasmContractClass>(read_json_file("casm.json")).unwrap();
    let compiled_class_hash = CompiledClassHash(StarkHash::default());

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            StateDiff {
                deployed_contracts: indexmap!(
                    address1 => class_hash1,
                    address2 => class_hash2
                ),
                storage_diffs: indexmap!(),
                declared_classes: indexmap!(
                    class_hash2 =>
                    (compiled_class_hash, class2)
                ),
                deprecated_declared_classes: indexmap!(
                    class_hash1 => class1
                ),
                nonces: indexmap!(
                    address1 => Nonce::default(),
                    address2 => Nonce::default()
                ),
                replaced_classes: indexmap!(),
            },
            indexmap!(),
        )
        .unwrap()
        .append_casm(&class_hash2, &casm)
        .unwrap()
        .commit()
        .unwrap();
}
