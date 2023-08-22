use std::collections::HashSet;
use std::net::SocketAddr;
use std::ops::Index;

use assert_matches::assert_matches;
use indexmap::IndexMap;
use itertools::Itertools;
use jsonrpsee::core::Error;
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::Methods;
use jsonschema::JSONSchema;
use papyrus_common::BlockHashAndNumber;
use papyrus_storage::base_layer::BaseLayerStorageWriter;
use papyrus_storage::body::events::EventIndex;
use papyrus_storage::body::{BodyStorageWriter, TransactionIndex};
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockStatus};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::{
    ContractClassAbiEntry,
    FunctionAbiEntry,
    FunctionStateMutability,
};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{StateDiff, StorageKey};
use starknet_api::transaction::{
    EventIndexInTransactionOutput,
    EventKey,
    TransactionHash,
    TransactionOffsetInBlock,
};
use starknet_api::{patricia_key, stark_felt};
use test_utils::{
    get_rng,
    get_test_block,
    get_test_body,
    get_test_state_diff,
    send_request,
    GetTestInstance,
};

use super::super::api::EventsChunk;
use super::super::block::Block;
use super::super::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use super::super::state::{ContractClass, StateUpdate, ThinStateDiff};
use super::super::transaction::{
    Event,
    TransactionOutput,
    TransactionReceipt,
    TransactionReceiptWithStatus,
    TransactionStatus,
    TransactionWithHash,
    Transactions,
};
use super::api_impl::{JsonRpcServerV0_3Impl, BLOCK_HASH_TABLE_ADDRESS};
use super::{ContinuationToken, EventFilter};
use crate::api::{BlockHashOrNumber, BlockId, Tag};
use crate::syncing_state::SyncStatus;
use crate::test_utils::{
    call_api_then_assert_and_validate_schema_for_err,
    call_api_then_assert_and_validate_schema_for_result,
    get_method_names_from_spec,
    get_starknet_spec_api_schema_for_components,
    get_starknet_spec_api_schema_for_method_results,
    get_test_gateway_config,
    get_test_highest_block,
    get_test_rpc_server_and_storage_writer,
    get_test_rpc_server_and_storage_writer_from_params,
    method_name_to_spec_method_name,
    raw_call,
    validate_schema,
    SpecFile,
};
use crate::v0_3_0::error::JsonRpcError;
use crate::version_config::VERSION_0_3;
use crate::{run_server, ContinuationTokenAsStruct};

const NODE_VERSION: &str = "NODE VERSION";

#[tokio::test]
async fn chain_id() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();

    // The result should be equal to the result of the following python code
    // hex(int.from_bytes(b'SN_GOERLI', byteorder="big", signed=False))
    // taken from starknet documentation:
    // https://docs.starknet.io/documentation/develop/Blocks/transactions/#chain-id.
    call_api_then_assert_and_validate_schema_for_result::<_, _, String>(
        &module,
        "starknet_V0_3_chainId",
        &None::<()>,
        &VERSION_0_3,
        &String::from("0x534e5f474f45524c49"),
    )
    .await;
}

#[tokio::test]
async fn block_hash_and_number() {
    let method_name = "starknet_V0_3_blockHashAndNumber";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();

    // No blocks yet.
    call_api_then_assert_and_validate_schema_for_err::<_, _, BlockHashAndNumber>(
        &module,
        method_name,
        &None::<()>,
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::NoBlocks as i32,
            JsonRpcError::NoBlocks.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Add a block and check again.
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .commit()
        .unwrap();
    call_api_then_assert_and_validate_schema_for_result::<_, _, BlockHashAndNumber>(
        &module,
        method_name,
        &None::<()>,
        &VERSION_0_3,
        &BlockHashAndNumber {
            block_hash: block.header.block_hash,
            block_number: block.header.block_number,
        },
    )
    .await;
}

#[tokio::test]
async fn block_number() {
    let method_name = "starknet_V0_3_blockNumber";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();

    // No blocks yet.
    let expected_err = ErrorObjectOwned::owned(
        JsonRpcError::NoBlocks as i32,
        JsonRpcError::NoBlocks.to_string(),
        None::<()>,
    );
    call_api_then_assert_and_validate_schema_for_err::<_, _, BlockNumber>(
        &module,
        method_name,
        &None::<()>,
        &VERSION_0_3,
        &expected_err,
    )
    .await;

    // Add a block and check again.
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .commit()
        .unwrap();
    call_api_then_assert_and_validate_schema_for_result::<_, _, BlockNumber>(
        &module,
        method_name,
        &None::<()>,
        &VERSION_0_3,
        &BlockNumber(0),
    )
    .await;
}

#[tokio::test]
async fn syncing() {
    const API_METHOD_NAME: &str = "starknet_V0_3_syncing";

    let shared_highest_block = get_test_highest_block();
    let (module, _) = get_test_rpc_server_and_storage_writer_from_params::<JsonRpcServerV0_3Impl>(
        None,
        Some(shared_highest_block.clone()),
    );

    call_api_then_assert_and_validate_schema_for_result::<_, _, bool>(
        &module,
        API_METHOD_NAME,
        &None::<()>,
        &VERSION_0_3,
        &false,
    )
    .await;

    *shared_highest_block.write().await =
        Some(BlockHashAndNumber { block_number: BlockNumber(5), ..Default::default() });
    call_api_then_assert_and_validate_schema_for_result::<_, _, SyncStatus>(
        &module,
        API_METHOD_NAME,
        &None::<()>,
        &VERSION_0_3,
        &SyncStatus { highest_block_num: BlockNumber(5), ..Default::default() },
    )
    .await;
}

#[tokio::test]
async fn get_block_transaction_count() {
    let method_name = "starknet_V0_3_getBlockTransactionCount";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<_, BlockId, usize>(
        &module,
        method_name,
        &Some(BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash))),
        &VERSION_0_3,
        &transaction_count,
    )
    .await;

    // Get block by number.
    let res = module
        .call::<_, usize>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(res, transaction_count);

    // Ask for the latest block.
    let res = module.call::<_, usize>(method_name, [BlockId::Tag(Tag::Latest)]).await.unwrap();
    assert_eq!(res, transaction_count);

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, BlockId, usize>(
        &module,
        method_name,
        &Some(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
            "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
        ))))),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, usize>(
            method_name,
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
    let method_name = "starknet_V0_3_getBlockWithTxs";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();

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
    call_api_then_assert_and_validate_schema_for_result::<_, BlockId, Block>(
        &module,
        method_name,
        &Some(BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))),
        &VERSION_0_3,
        &expected_block,
    )
    .await;

    // Get block by number.
    let block = module
        .call::<_, Block>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module.call::<_, Block>(method_name, [BlockId::Tag(Tag::Latest)]).await.unwrap();
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
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block.status, BlockStatus::AcceptedOnL1);

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Block>(
            method_name,
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
            method_name,
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
    let method_name = "starknet_V0_3_getBlockWithTxHashes";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();

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
    call_api_then_assert_and_validate_schema_for_result::<_, BlockId, Block>(
        &module,
        method_name,
        &Some(BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))),
        &VERSION_0_3,
        &expected_block,
    )
    .await;

    // Get block by number.
    let block = module
        .call::<_, Block>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block.header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(block, expected_block);

    // Ask for the latest block.
    let block = module.call::<_, Block>(method_name, [BlockId::Tag(Tag::Latest)]).await.unwrap();
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
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block.header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block.status, BlockStatus::AcceptedOnL1);

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, BlockId, Block>(
        &module,
        method_name,
        &Some(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
            "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
        ))))),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, Block>(
            method_name,
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
    let method_name = "starknet_V0_3_getClass";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<
        _,
        (BlockId, ClassHash),
        DeprecatedContractClass,
    >(
        &module,
        method_name,
        &Some((BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *class_hash)),
        &VERSION_0_3,
        &expected_contract_class,
    )
    .await;

    // Get class by block number.
    let res = module
        .call::<_, DeprecatedContractClass>(
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Ask for an invalid class hash.
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        (BlockId, ClassHash),
        DeprecatedContractClass,
    >(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
            ClassHash(stark_felt!("0x7")),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::ClassHashNotFound as i32,
            JsonRpcError::ClassHashNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // New Class
    let (class_hash, (_compiled_class_hash, contract_class)) =
        diff.declared_classes.get_index(0).unwrap();
    let expected_contract_class = contract_class.clone().into();

    // Get class by block hash.
    let res = module
        .call::<_, ContractClass>(
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, ContractClass>(
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Invalid Call
    // Ask for an invalid class hash in the given block.
    let err = module
        .call::<_, DeprecatedContractClass>(
            method_name,
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
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        (BlockId, ClassHash),
        DeprecatedContractClass,
    >(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            )))),
            ClassHash(stark_felt!("0x7")),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, DeprecatedContractClass>(
            method_name,
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
    let method_name = "starknet_V0_3_getTransactionReceipt";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
        status: TransactionStatus::AcceptedOnL2,
    };
    let (json_response, res) = raw_call::<_, TransactionHash, TransactionReceiptWithStatus>(
        &module,
        method_name,
        &Some(transaction_hash),
    )
    .await;
    // The returned jsons of some transaction outputs are the same. When deserialized, the first
    // struct in the TransactionOutput enum that matches the json is chosen. To not depend here
    // on the order of structs we compare the serialized data.
    assert_eq!(
        serde_json::to_string(&res.clone().unwrap()).unwrap(),
        serde_json::to_string(&expected_receipt).unwrap(),
    );
    assert!(validate_schema(
        &get_starknet_spec_api_schema_for_method_results(
            &[(
                SpecFile::StarknetApiOpenrpc,
                &[method_name_to_spec_method_name(method_name).as_str()]
            )],
            &VERSION_0_3,
        ),
        &json_response["result"],
    ));

    // Ask for a transaction in a block that was accepted on L1.
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_block_marker(&block.header.block_number.next())
        .unwrap()
        .commit()
        .unwrap();
    let res = module
        .call::<_, TransactionReceiptWithStatus>(method_name, [transaction_hash])
        .await
        .unwrap();
    assert_eq!(res.status, TransactionStatus::AcceptedOnL1);

    // Ask for an invalid transaction.
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        TransactionHash,
        TransactionReceiptWithStatus,
    >(
        &module,
        method_name,
        &Some(TransactionHash(StarkHash::from(1_u8))),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::TransactionHashNotFound as i32,
            JsonRpcError::TransactionHashNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;
}

#[tokio::test]
async fn get_class_at() {
    let method_name = "starknet_V0_3_getClassAt";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<
        _,
        (BlockId, ContractAddress),
        DeprecatedContractClass,
    >(
        &module,
        method_name,
        &Some((BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address)),
        &VERSION_0_3,
        &expected_contract_class,
    )
    .await;

    // Get class by block number.
    let res = module
        .call::<_, DeprecatedContractClass>(
            method_name,
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
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Get class by block number.
    let res = module
        .call::<_, ContractClass>(
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_contract_class);

    // Invalid Call
    // Ask for an invalid contract.
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        (BlockId, ContractAddress),
        DeprecatedContractClass,
    >(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
            ContractAddress(patricia_key!("0x12")),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::ContractNotFound as i32,
            JsonRpcError::ContractNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid contract in the given block.
    let err = module
        .call::<_, DeprecatedContractClass>(
            method_name,
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
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        (BlockId, ContractAddress),
        DeprecatedContractClass,
    >(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            )))),
            *address,
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, DeprecatedContractClass>(
            method_name,
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
    let method_name = "starknet_V0_3_getClassHashAt";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<_, (BlockId, ContractAddress), ClassHash>(
        &module,
        method_name,
        &Some((BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address)),
        &VERSION_0_3,
        expected_class_hash,
    )
    .await;

    // Get class hash by block number.
    let res = module
        .call::<_, ClassHash>(
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_class_hash);

    // Ask for an invalid contract.
    call_api_then_assert_and_validate_schema_for_err::<_, (BlockId, ContractAddress), ClassHash>(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
            ContractAddress(patricia_key!("0x12")),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::ContractNotFound as i32,
            JsonRpcError::ContractNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, (BlockId, ContractAddress), ClassHash>(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            )))),
            *address,
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, ClassHash>(
            method_name,
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
    let method_name = "starknet_V0_3_getNonce";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<_, (BlockId, ContractAddress), Nonce>(
        &module,
        method_name,
        &Some((BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)), *address)),
        &VERSION_0_3,
        expected_nonce,
    )
    .await;

    // Get class hash by block number.
    let res = module
        .call::<_, Nonce>(
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_nonce);

    // Ask for an invalid contract.
    call_api_then_assert_and_validate_schema_for_err::<_, (BlockId, ContractAddress), Nonce>(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
            ContractAddress(patricia_key!("0x31")),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::ContractNotFound as i32,
            JsonRpcError::ContractNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, (BlockId, ContractAddress), Nonce>(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            )))),
            *address,
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, Nonce>(
            method_name,
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
    let method_name = "starknet_V0_3_getStorageAt";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<
        _,
        (ContractAddress, StorageKey, BlockId),
        StarkFelt,
    >(
        &module,
        method_name,
        &Some((*address, *key, BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)))),
        &VERSION_0_3,
        expected_value,
    )
    .await;

    // Get storage by block number.
    let res = module
        .call::<_, StarkFelt>(
            method_name,
            (*address, *key, BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Ask for storage at address 1 - the block hash table contract address
    let key = StorageKey(patricia_key!("0x1001"));
    let res = module
        .call::<_, StarkFelt>(
            "starknet_V0_3_getStorageAt",
            (
                *BLOCK_HASH_TABLE_ADDRESS,
                key,
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
            ),
        )
        .await
        .unwrap();
    assert_eq!(res, StarkFelt::default());

    // Ask for an invalid contract.
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        (ContractAddress, StorageKey, BlockId),
        StarkFelt,
    >(
        &module,
        method_name,
        &Some((
            ContractAddress(patricia_key!("0x12")),
            key,
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::ContractNotFound as i32,
            JsonRpcError::ContractNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        (ContractAddress, StorageKey, BlockId),
        StarkFelt,
    >(
        &module,
        method_name,
        &Some((
            *address,
            key,
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            )))),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, StarkFelt>(
            method_name,
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
    let method_name = "starknet_V0_3_getTransactionByHash";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<_, TransactionHash, TransactionWithHash>(
        &module,
        method_name,
        &Some(block.body.transaction_hashes[0]),
        &VERSION_0_3,
        &expected_transaction,
    )
    .await;

    // Ask for an invalid transaction.
    call_api_then_assert_and_validate_schema_for_err::<_, TransactionHash, TransactionWithHash>(
        &module,
        method_name,
        &Some(TransactionHash(StarkHash::from(1_u8))),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::TransactionHashNotFound as i32,
            JsonRpcError::TransactionHashNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;
}

#[tokio::test]
async fn get_transaction_by_block_id_and_index() {
    let method_name = "starknet_V0_3_getTransactionByBlockIdAndIndex";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<
        _,
        (BlockId, TransactionOffsetInBlock),
        TransactionWithHash,
    >(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash)),
            TransactionOffsetInBlock(0),
        )),
        &VERSION_0_3,
        &expected_transaction,
    )
    .await;

    // Get transaction by block number.
    let res = module
        .call::<_, TransactionWithHash>(
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(block.header.block_number)), 0),
        )
        .await
        .unwrap();
    assert_eq!(res, expected_transaction);

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        (BlockId, TransactionOffsetInBlock),
        TransactionWithHash,
    >(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
                "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
            )))),
            TransactionOffsetInBlock(0),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, TransactionWithHash>(
            method_name,
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
    call_api_then_assert_and_validate_schema_for_err::<
        _,
        (BlockId, TransactionOffsetInBlock),
        TransactionWithHash,
    >(
        &module,
        method_name,
        &Some((
            BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash)),
            TransactionOffsetInBlock(1),
        )),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::InvalidTransactionIndex as i32,
            JsonRpcError::InvalidTransactionIndex.to_string(),
            None::<()>,
        ),
    )
    .await;
}

#[tokio::test]
async fn get_state_update() {
    let method_name = "starknet_V0_3_getStateUpdate";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
    call_api_then_assert_and_validate_schema_for_result::<_, BlockId, StateUpdate>(
        &module,
        method_name,
        &Some(BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
        &VERSION_0_3,
        &expected_update,
    )
    .await;

    // Get state update by block number.
    let res = module
        .call::<_, StateUpdate>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))],
        )
        .await
        .unwrap();
    assert_eq!(res, expected_update);

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, BlockId, StateUpdate>(
        &module,
        method_name,
        &Some(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(stark_felt!(
            "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484"
        ))))),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::BlockNotFound as i32,
            JsonRpcError::BlockNotFound.to_string(),
            None::<()>,
        ),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, StateUpdate>(
            method_name,
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
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
            .call::<_, EventsChunk>("starknet_V0_3_getEvents", [filter.clone()])
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
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
            .call::<_, EventsChunk>("starknet_V0_3_getEvents", [filter.clone()])
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
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: get_test_gateway_config().max_events_chunk_size + 1,
        address: None,
        keys: vec![],
    };

    call_api_then_assert_and_validate_schema_for_err::<_, EventFilter, EventsChunk>(
        &module,
        "starknet_V0_3_getEvents",
        &Some(filter),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::PageSizeTooBig as i32,
            JsonRpcError::PageSizeTooBig.to_string(),
            None::<()>,
        ),
    )
    .await;
}

#[tokio::test]
async fn get_events_too_many_keys() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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

    call_api_then_assert_and_validate_schema_for_err::<_, EventFilter, EventsChunk>(
        &module,
        "starknet_V0_3_getEvents",
        &Some(filter),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::TooManyKeysInFilter as i32,
            JsonRpcError::TooManyKeysInFilter.to_string(),
            None::<()>,
        ),
    )
    .await;
}

// TODO(nevo): add a test that returns the bock not found error for getEvents
#[tokio::test]
async fn get_events_no_blocks() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: 2,
        address: None,
        keys: vec![],
    };

    let (json_response, res) =
        raw_call::<_, EventFilter, EventsChunk>(&module, "starknet_V0_3_getEvents", &Some(filter))
            .await;
    assert!(
        &json_response
            .get("result")
            .expect("response should have result field")
            .get("continuation_token")
            .is_none()
    );
    assert_eq!(res.unwrap(), EventsChunk { events: vec![], continuation_token: None });
}

#[tokio::test]
async fn get_events_no_blocks_in_filter() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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

    call_api_then_assert_and_validate_schema_for_result::<_, EventFilter, EventsChunk>(
        &module,
        "starknet_V0_3_getEvents",
        &Some(filter),
        &VERSION_0_3,
        &EventsChunk { events: vec![], continuation_token: None },
    )
    .await;
}

#[tokio::test]
async fn get_events_invalid_ct() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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

    call_api_then_assert_and_validate_schema_for_err::<_, EventFilter, EventsChunk>(
        &module,
        "starknet_V0_3_getEvents",
        &Some(filter),
        &VERSION_0_3,
        &ErrorObjectOwned::owned(
            JsonRpcError::InvalidContinuationToken as i32,
            JsonRpcError::InvalidContinuationToken.to_string(),
            None::<()>,
        ),
    )
    .await;
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
        run_server(&gateway_config, get_test_highest_block(), storage_reader, NODE_VERSION)
            .await
            .unwrap();

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
        &VERSION_0_3,
    );
    validate_state(&state_diff, server_address, &schema).await;
    validate_block(&block.header, server_address, &schema).await;
    validate_transaction(block.body.transaction_hashes.index(0), server_address, &schema).await;
}

async fn validate_state(state_diff: &StateDiff, server_address: SocketAddr, schema: &JSONSchema) {
    let res = send_request(
        server_address,
        "starknet_getStateUpdate",
        r#"{"block_number": 1}"#,
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "State update is not valid.");

    let (address, _) = state_diff.deployed_contracts.get_index(0).unwrap();
    let res = send_request(
        server_address,
        "starknet_getClassAt",
        format!(r#"{{"block_number": 1}}, "0x{}""#, hex::encode(address.0.key().bytes())).as_str(),
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Class is not valid.");

    // TODO(dvir): Remove this after regenesis.
    // This checks the deployed deprecated class.
    let (address, _) = state_diff.deployed_contracts.get_index(1).unwrap();
    let res = send_request(
        server_address,
        "starknet_getClassAt",
        format!(r#"{{"block_number": 1}}, "0x{}""#, hex::encode(address.0.key().bytes())).as_str(),
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Class is not valid.");
}

async fn validate_block(header: &BlockHeader, server_address: SocketAddr, schema: &JSONSchema) {
    let res = send_request(
        server_address,
        "starknet_getBlockWithTxs",
        r#"{"block_number": 1}"#,
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Block with transactions is not valid.");

    let res = send_request(
        server_address,
        "starknet_getBlockWithTxHashes",
        format!(r#"{{"block_hash": "0x{}"}}"#, hex::encode(header.block_hash.0.bytes())).as_str(),
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Block with transaction hashes is not valid.");
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
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Transaction is not valid.");

    let res = send_request(
        server_address,
        "starknet_getTransactionByHash",
        format!(r#""0x{}""#, hex::encode(tx_hash.0.bytes())).as_str(),
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Transaction is not valid.");

    let res = send_request(
        server_address,
        "starknet_getTransactionReceipt",
        format!(r#""0x{}""#, hex::encode(tx_hash.0.bytes())).as_str(),
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Transaction receipt is not valid.");

    let res = send_request(
        server_address,
        "starknet_getEvents",
        r#"{"chunk_size": 2}"#,
        VERSION_0_3.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Events are not valid.");
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
        ContractClassAbiEntry::Function(function_abi_without_state_mutability);
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
        ContractClassAbiEntry::Function(function_abi_with_state_mutability);
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
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
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
            "starknet_V0_3_getClass",
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
            "starknet_V0_3_getClass",
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

#[test]
fn spec_api_methods_coverage() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_3Impl>();
    let implemented_methods: Methods = module.into();
    let implemented_method_names = implemented_methods
        .method_names()
        .map(method_name_to_spec_method_name)
        .sorted()
        .collect::<Vec<_>>();
    for mn in implemented_method_names.clone() {
        println!("{}", mn);
    }
    let non_implemented_apis = [
        "starknet_addDeclareTransaction".to_string(),
        "starknet_addDeployAccountTransaction".to_string(),
        "starknet_addInvokeTransaction".to_string(),
        "starknet_call".to_string(),
        "starknet_estimateFee".to_string(),
        "starknet_estimateMessageFee".to_string(),
        "starknet_pendingTransactions".to_string(),
        "starknet_traceBlockTransactions".to_string(),
        "starknet_simulateTransaction".to_string(),
        "starknet_traceTransaction".to_string(),
    ];
    let method_names_in_spec = get_method_names_from_spec(&VERSION_0_3)
        .iter()
        .filter_map(|method| {
            let stripped_method_name = method.clone().replace('\"', "");
            if non_implemented_apis.contains(&stripped_method_name) {
                None
            } else {
                Some(stripped_method_name)
            }
        })
        .sorted()
        .collect::<Vec<_>>();
    for mn in method_names_in_spec.clone() {
        println!("{}", mn);
    }
    assert!(method_names_in_spec.eq(&implemented_method_names));
}
