use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::iter;
use std::net::SocketAddr;
use std::ops::Index;

use assert_matches::assert_matches;
use async_trait::async_trait;
use indexmap::{indexmap, IndexMap};
use itertools::Itertools;
use jsonrpsee::core::Error;
use jsonrpsee::Methods;
use jsonschema::JSONSchema;
use lazy_static::lazy_static;
use mockall::predicate::eq;
use papyrus_common::pending_classes::{ApiContractClass, PendingClassesTrait};
use papyrus_common::BlockHashAndNumber;
use papyrus_storage::base_layer::BaseLayerStorageWriter;
use papyrus_storage::body::events::EventIndex;
use papyrus_storage::body::{BodyStorageWriter, TransactionIndex};
use papyrus_storage::header::{HeaderStorageWriter, StarknetVersion};
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageScope;
use pretty_assertions::assert_eq;
use rand::{random, RngCore};
use rand_chacha::ChaCha8Rng;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use starknet_api::block::{
    Block as StarknetApiBlock,
    BlockHash,
    BlockHeader,
    BlockNumber,
    BlockStatus,
    BlockTimestamp,
    GasPrice,
};
use starknet_api::core::{ClassHash, ContractAddress, GlobalRoot, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::{
    ContractClassAbiEntry,
    FunctionAbiEntry,
    FunctionStateMutability,
};
use starknet_api::hash::GENESIS_HASH;
use starknet_api::patricia_key;
use starknet_api::state::{ContractClass as StarknetApiContractClass, StateDiff, StorageKey};
use starknet_api::transaction::{
    Event as StarknetApiEvent,
    EventContent,
    EventData,
    EventIndexInTransactionOutput,
    EventKey,
    Transaction as StarknetApiTransaction,
    TransactionExecutionStatus,
    TransactionHash,
    TransactionOffsetInBlock,
    TransactionOutput as StarknetApiTransactionOutput,
};
use starknet_client::reader::objects::pending_data::{
    PendingBlock,
    PendingStateUpdate as ClientPendingStateUpdate,
};
use starknet_client::reader::objects::state::{
    DeclaredClassHashEntry as ClientDeclaredClassHashEntry,
    DeployedContract as ClientDeployedContract,
    ReplacedClass as ClientReplacedClass,
    StateDiff as ClientStateDiff,
    StorageEntry as ClientStorageEntry,
};
use starknet_client::reader::objects::transaction::{
    Transaction as ClientTransaction,
    TransactionReceipt as ClientTransactionReceipt,
};
use starknet_client::starknet_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};
use starknet_client::writer::objects::response::{
    DeclareResponse,
    DeployAccountResponse,
    InvokeResponse,
};
use starknet_client::writer::objects::transaction::{
    DeclareTransaction as ClientDeclareTransaction,
    DeployAccountTransaction as ClientDeployAccountTransaction,
    InvokeTransaction as ClientInvokeTransaction,
};
use starknet_client::writer::{MockStarknetWriter, WriterClientError, WriterClientResult};
use starknet_client::ClientError;
use starknet_types_core::felt::Felt;
use test_utils::{
    auto_impl_get_test_instance,
    get_number_of_variants,
    get_rng,
    get_test_block,
    get_test_body,
    get_test_state_diff,
    send_request,
    GetTestInstance,
};

use super::super::api::EventsChunk;
use super::super::block::{Block, GeneralBlockHeader, PendingBlockHeader, ResourcePrice};
use super::super::broadcasted_transaction::BroadcastedDeclareTransaction;
use super::super::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use super::super::error::{
    unexpected_error,
    JsonRpcError,
    BLOCK_NOT_FOUND,
    CLASS_HASH_NOT_FOUND,
    COMPILATION_FAILED,
    CONTRACT_NOT_FOUND,
    DUPLICATE_TX,
    INVALID_CONTINUATION_TOKEN,
    INVALID_TRANSACTION_INDEX,
    NO_BLOCKS,
    PAGE_SIZE_TOO_BIG,
    TOO_MANY_KEYS_IN_FILTER,
    TRANSACTION_HASH_NOT_FOUND,
};
use super::super::state::{
    AcceptedStateUpdate,
    ClassHashes,
    ContractClass,
    ContractNonce,
    DeployedContract,
    PendingStateUpdate,
    ReplacedClasses,
    StateUpdate,
    StorageDiff,
    StorageEntry,
    ThinStateDiff,
};
use super::super::transaction::{
    DeployAccountTransaction,
    Event,
    GeneralTransactionReceipt,
    InvokeTransaction,
    L1HandlerMsgHash,
    L1L2MsgHash,
    PendingTransactionFinalityStatus,
    PendingTransactionOutput,
    PendingTransactionReceipt,
    TransactionFinalityStatus,
    TransactionOutput,
    TransactionReceipt,
    TransactionStatus,
    TransactionWithHash,
    Transactions,
    TypedDeployAccountTransaction,
    TypedInvokeTransaction,
};
use super::super::write_api_result::{
    AddDeclareOkResult,
    AddDeployAccountOkResult,
    AddInvokeOkResult,
};
use super::api_impl::{JsonRpcServerV0_6Impl as JsonRpcServerImpl, BLOCK_HASH_TABLE_ADDRESS};
use super::{ContinuationToken, EventFilter, GatewayContractClass};
use crate::api::{BlockHashOrNumber, BlockId, Tag};
use crate::syncing_state::SyncStatus;
use crate::test_utils::{
    call_api_then_assert_and_validate_schema_for_err,
    call_api_then_assert_and_validate_schema_for_result,
    get_method_names_from_spec,
    get_starknet_spec_api_schema_for_components,
    get_starknet_spec_api_schema_for_method_results,
    get_test_highest_block,
    get_test_pending_classes,
    get_test_pending_data,
    get_test_rpc_config,
    get_test_rpc_server_and_storage_writer,
    get_test_rpc_server_and_storage_writer_from_params,
    method_name_to_spec_method_name,
    raw_call,
    validate_schema,
    SpecFile,
};
use crate::version_config::VERSION_0_6 as VERSION;
use crate::{
    internal_server_error,
    internal_server_error_with_msg,
    run_server,
    ContinuationTokenAsStruct,
};

const NODE_VERSION: &str = "NODE VERSION";

#[tokio::test]
async fn spec_version() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    call_api_then_assert_and_validate_schema_for_result(
        &module,
        "starknet_V0_6_specVersion",
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &format!("{VERSION}"),
    )
    .await;
}

#[tokio::test]
async fn chain_id() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    // The result should be equal to the result of the following python code
    // hex(int.from_bytes(b'SN_GOERLI', byteorder="big", signed=False))
    // taken from starknet documentation:
    // https://docs.starknet.io/documentation/develop/Blocks/transactions/#chain-id.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        "starknet_V0_6_chainId",
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &String::from("0x534e5f474f45524c49"),
    )
    .await;
}

#[tokio::test]
async fn block_hash_and_number() {
    let method_name = "starknet_V0_6_blockHashAndNumber";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    // No blocks yet.
    call_api_then_assert_and_validate_schema_for_err::<_, BlockHashAndNumber>(
        &module,
        method_name,
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &NO_BLOCKS.into(),
    )
    .await;

    // Add a block without state diff and check that there are still no blocks.
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .update_starknet_version(&block.header.block_number, &StarknetVersion::default())
        .unwrap()
        .commit()
        .unwrap();
    call_api_then_assert_and_validate_schema_for_err::<_, BlockHashAndNumber>(
        &module,
        method_name,
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &NO_BLOCKS.into(),
    )
    .await;

    // Add a state diff to the block and check that we get the block.
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(
            block.header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .commit()
        .unwrap();
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BlockHashAndNumber {
            block_hash: block.header.block_hash,
            block_number: block.header.block_number,
        },
    )
    .await;
}

#[tokio::test]
async fn block_number() {
    let method_name = "starknet_V0_6_blockNumber";
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    // No blocks yet.
    let expected_err = NO_BLOCKS.into();
    call_api_then_assert_and_validate_schema_for_err::<_, BlockNumber>(
        &module,
        method_name,
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_err,
    )
    .await;

    // Add a block without state diff and check that there are still no blocks.
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .update_starknet_version(&BlockNumber(0), &StarknetVersion::default())
        .unwrap()
        .commit()
        .unwrap();
    call_api_then_assert_and_validate_schema_for_err::<_, BlockNumber>(
        &module,
        method_name,
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_err,
    )
    .await;

    // Add a state diff to the block and check that we get the block.
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .commit()
        .unwrap();
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BlockNumber(0),
    )
    .await;
}

#[tokio::test]
async fn syncing() {
    const API_METHOD_NAME: &str = "starknet_V0_6_syncing";

    let shared_highest_block = get_test_highest_block();
    let (module, _) = get_test_rpc_server_and_storage_writer_from_params::<JsonRpcServerImpl>(
        None,
        Some(shared_highest_block.clone()),
        None,
        None,
        None,
    );

    call_api_then_assert_and_validate_schema_for_result(
        &module,
        API_METHOD_NAME,
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &false,
    )
    .await;

    *shared_highest_block.write().await =
        Some(BlockHashAndNumber { block_number: BlockNumber(5), ..Default::default() });
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        API_METHOD_NAME,
        vec![],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &SyncStatus { highest_block_num: BlockNumber(5), ..Default::default() },
    )
    .await;
}

#[tokio::test]
async fn get_block_transaction_count() {
    let method_name = "starknet_V0_6_getBlockTransactionCount";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let transaction_count = 5;
    let block = get_test_block(transaction_count, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .update_starknet_version(&block.header.block_number, &StarknetVersion::default())
        .unwrap()
        .append_body(block.header.block_number, block.body)
        .unwrap()
        .append_state_diff(
            block.header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .commit()
        .unwrap();

    // Get block by hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash)))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
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

    // Ask for pending block
    let pending_transaction_count = 3;
    let mut rng = get_rng();
    pending_data.write().await.block.transactions.extend(
        iter::repeat(ClientTransaction::get_test_instance(&mut rng))
            .take(pending_transaction_count),
    );
    let res = module.call::<_, usize>(method_name, [BlockId::Tag(Tag::Pending)]).await.unwrap();
    assert_eq!(res, pending_transaction_count);

    // Ask for pending block when it's not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    let res = module.call::<_, usize>(method_name, [BlockId::Tag(Tag::Pending)]).await.unwrap();
    assert_eq!(res, 0);

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, usize>(
        &module,
        method_name,
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
            Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                .unwrap(),
        ))))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());
}

#[tokio::test]
async fn get_block_w_full_transactions() {
    // TODO(omri): Add test for pending block.
    let method_name = "starknet_V0_6_getBlockWithTxs";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);

    let mut block = get_test_block(1, None, None, None);
    let block_hash = BlockHash(random::<u64>().into());
    let sequencer_address: ContractAddress = random::<u64>().into();
    let timestamp = BlockTimestamp(random::<u64>());
    let starknet_version = StarknetVersion("test".to_owned());
    block.header.block_hash = block_hash;
    block.header.sequencer = sequencer_address;
    block.header.timestamp = timestamp;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .update_starknet_version(&block.header.block_number, &starknet_version)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .append_state_diff(
            block.header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = TransactionWithHash {
        transaction: block.body.transactions[0].clone().try_into().unwrap(),
        transaction_hash: block.body.transaction_hashes[0],
    };
    let expected_block = Block {
        status: Some(BlockStatus::AcceptedOnL2),
        header: GeneralBlockHeader::BlockHeader((block.header, starknet_version.clone()).into()),
        transactions: Transactions::Full(vec![expected_transaction]),
    };
    let GeneralBlockHeader::BlockHeader(expected_block_header) = expected_block.clone().header
    else {
        panic!("Unexpected block_header type. Expected BlockHeader.");
    };

    // Get block by hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(
            expected_block_header.block_hash,
        )))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_block,
    )
    .await;

    // Get block by number.
    let block = module
        .call::<_, Block>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block_header.block_number))],
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
        .update_base_layer_block_marker(&expected_block_header.block_number.next())
        .unwrap()
        .commit()
        .unwrap();
    let block = module
        .call::<_, Block>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block_header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block.status, Some(BlockStatus::AcceptedOnL1));

    // Ask for an invalid block hash.
    let err = module
        .call::<_, Block>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
                Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                    .unwrap(),
            )))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());

    // Ask for an invalid block number.
    let err = module
        .call::<_, Block>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))],
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());

    // Get pending block.
    let mut rng = get_rng();
    let (client_transactions, rpc_transactions): (Vec<_>, Vec<_>) =
        iter::repeat_with(|| generate_client_transaction_and_rpc_transaction(&mut rng))
            .take(3)
            .unzip();
    let pending_sequencer_address: ContractAddress = random::<u64>().into();
    let pending_timestamp = BlockTimestamp(random::<u64>());
    let pending_eth_l1_gas_price = GasPrice(random::<u128>());
    let pending_strk_l1_gas_price = GasPrice(random::<u128>());
    let expected_pending_block = Block {
        header: GeneralBlockHeader::PendingBlockHeader(PendingBlockHeader {
            parent_hash: block_hash,
            sequencer_address: pending_sequencer_address,
            timestamp: pending_timestamp,
            l1_gas_price: ResourcePrice {
                price_in_wei: pending_eth_l1_gas_price,
                price_in_fri: pending_strk_l1_gas_price,
            },
            starknet_version: starknet_version.0.clone(),
        }),
        status: None,
        transactions: Transactions::Full(rpc_transactions),
    };
    {
        let pending_block = &mut pending_data.write().await.block;

        pending_block.transactions.extend(client_transactions);
        pending_block.parent_block_hash = block_hash;
        pending_block.timestamp = pending_timestamp;
        pending_block.sequencer_address = pending_sequencer_address;
        pending_block.eth_l1_gas_price = pending_eth_l1_gas_price;
        pending_block.strk_l1_gas_price = pending_strk_l1_gas_price;
        pending_block.starknet_version = starknet_version.0;
    }
    // Using call_api_then_assert_and_validate_schema_for_result in order to validate the schema for
    // pending block.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_pending_block,
    )
    .await;

    // Get pending block when it's not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    let res_block =
        module.call::<_, Block>(method_name, [BlockId::Tag(Tag::Pending)]).await.unwrap();
    let GeneralBlockHeader::PendingBlockHeader(pending_block_header) = res_block.header else {
        panic!("Unexpected block_header type. Expected PendingBlockHeader.")
    };
    assert_eq!(pending_block_header.parent_hash, block_hash);
    assert_eq!(pending_block_header.sequencer_address, sequencer_address);
    assert_eq!(pending_block_header.timestamp, timestamp);
    match res_block.transactions {
        Transactions::Hashes(transactions) => assert_eq!(transactions.len(), 0),
        Transactions::Full(transactions) => assert_eq!(transactions.len(), 0),
    };
}

#[tokio::test]
async fn get_block_w_transaction_hashes() {
    let method_name = "starknet_V0_6_getBlockWithTxHashes";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);

    let mut block = get_test_block(1, None, None, None);
    let block_hash = BlockHash(random::<u64>().into());
    let sequencer_address: ContractAddress = random::<u64>().into();
    let timestamp = BlockTimestamp(random::<u64>());
    let starknet_version = StarknetVersion("test".to_owned());
    block.header.block_hash = block_hash;
    block.header.sequencer = sequencer_address;
    block.header.timestamp = timestamp;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .update_starknet_version(&block.header.block_number, &starknet_version)
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .append_state_diff(
            block.header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .commit()
        .unwrap();

    let expected_block = Block {
        status: Some(BlockStatus::AcceptedOnL2),
        header: GeneralBlockHeader::BlockHeader((block.header, starknet_version.clone()).into()),
        transactions: Transactions::Hashes(vec![block.body.transaction_hashes[0]]),
    };
    let GeneralBlockHeader::BlockHeader(expected_block_header) = expected_block.clone().header
    else {
        panic!("Unexpected block_header type. Expected BlockHeader.");
    };

    // Get block by hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(
            expected_block_header.block_hash,
        )))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_block,
    )
    .await;

    // Get block by number.
    let block = module
        .call::<_, Block>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(expected_block_header.block_number))],
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
        .update_base_layer_block_marker(&expected_block_header.block_number.next())
        .unwrap()
        .commit()
        .unwrap();
    let block = module
        .call::<_, Block>(
            method_name,
            [BlockId::HashOrNumber(BlockHashOrNumber::Hash(expected_block_header.block_hash))],
        )
        .await
        .unwrap();
    assert_eq!(block.status, Some(BlockStatus::AcceptedOnL1));

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, Block>(
        &module,
        method_name,
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
            Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                .unwrap(),
        ))))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());

    // Get pending block.
    let mut rng = get_rng();
    let (client_transactions, _): (Vec<_>, Vec<_>) =
        iter::repeat_with(|| generate_client_transaction_and_rpc_transaction(&mut rng))
            .take(3)
            .unzip();
    let pending_sequencer_address: ContractAddress = random::<u64>().into();
    let pending_timestamp = BlockTimestamp(random::<u64>());
    let pending_eth_l1_gas_price = GasPrice(random::<u128>());
    let pending_strk_l1_gas_price = GasPrice(random::<u128>());
    let expected_pending_block = Block {
        header: GeneralBlockHeader::PendingBlockHeader(PendingBlockHeader {
            parent_hash: block_hash,
            sequencer_address: pending_sequencer_address,
            timestamp: pending_timestamp,
            l1_gas_price: ResourcePrice {
                price_in_wei: pending_eth_l1_gas_price,
                price_in_fri: pending_strk_l1_gas_price,
            },
            starknet_version: starknet_version.0.clone(),
        }),
        status: None,
        transactions: Transactions::Hashes(
            client_transactions
                .iter()
                .map(|client_transaction| client_transaction.transaction_hash())
                .collect(),
        ),
    };
    {
        let pending_block = &mut pending_data.write().await.block;

        pending_block.transactions.extend(client_transactions);
        pending_block.parent_block_hash = block_hash;
        pending_block.timestamp = pending_timestamp;
        pending_block.sequencer_address = pending_sequencer_address;
        pending_block.eth_l1_gas_price = pending_eth_l1_gas_price;
        pending_block.strk_l1_gas_price = pending_strk_l1_gas_price;
        pending_block.starknet_version = starknet_version.0;
    }
    // Using call_api_then_assert_and_validate_schema_for_result in order to validate the schema for
    // pending block.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_pending_block,
    )
    .await;

    // Get pending block when it's not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    let res_block =
        module.call::<_, Block>(method_name, [BlockId::Tag(Tag::Pending)]).await.unwrap();
    let GeneralBlockHeader::PendingBlockHeader(pending_block_header) = res_block.header else {
        panic!("Unexpected block_header type. Expected PendingBlockHeader.")
    };
    assert_eq!(pending_block_header.parent_hash, block_hash);
    assert_eq!(pending_block_header.sequencer_address, sequencer_address);
    assert_eq!(pending_block_header.timestamp, timestamp);
    match res_block.transactions {
        Transactions::Hashes(transactions) => assert_eq!(transactions.len(), 0),
        Transactions::Full(transactions) => assert_eq!(transactions.len(), 0),
    };
}

#[tokio::test]
async fn get_class() {
    let method_name = "starknet_V0_6_getClass";
    let pending_classes = get_test_pending_classes();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, None, Some(pending_classes.clone()), None);
    let parent_header = BlockHeader::default();
    let header = BlockHeader {
        block_hash: BlockHash(Felt::ONE),
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
        .update_starknet_version(&parent_header.block_number, &StarknetVersion::default())
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
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
            Box::new(*class_hash),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
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

    // Get class of pending block
    let pending_class_hash = ClassHash(random::<u64>().into());
    let pending_class = ApiContractClass::ContractClass(
        StarknetApiContractClass::get_test_instance(&mut get_rng()),
    );
    pending_classes.write().await.add_class(pending_class_hash, pending_class.clone());
    let res = module
        .call::<_, GatewayContractClass>(
            method_name,
            (BlockId::Tag(Tag::Pending), pending_class_hash),
        )
        .await
        .unwrap();
    assert_eq!(res, pending_class.try_into().unwrap());

    // Ask for an invalid class hash.
    call_api_then_assert_and_validate_schema_for_err::<_, DeprecatedContractClass>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
            Box::new(ClassHash(Felt::from_hex_unchecked("0x7"))),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CLASS_HASH_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == CLASS_HASH_NOT_FOUND.into());

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, DeprecatedContractClass>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
                Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                    .unwrap(),
            )))),
            Box::new(ClassHash(Felt::from_hex_unchecked("0x7"))),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());
}

#[tokio::test]
async fn get_transaction_status() {
    let method_name = "starknet_V0_6_getTransactionStatus";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .update_starknet_version(&block.header.block_number, &StarknetVersion::default())
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let transaction_hash = block.body.transaction_hashes[0];
    let transaction_version = match block.body.transactions.index(0) {
        StarknetApiTransaction::Declare(tx) => tx.version(),
        StarknetApiTransaction::Deploy(tx) => tx.version,
        StarknetApiTransaction::DeployAccount(tx) => tx.version(),
        StarknetApiTransaction::Invoke(tx) => tx.version(),
        StarknetApiTransaction::L1Handler(tx) => tx.version,
    };
    let tx = block.body.transaction_outputs.index(0).clone();
    let msg_hash = match tx {
        starknet_api::transaction::TransactionOutput::L1Handler(_) => Some(L1L2MsgHash::default()),
        _ => None,
    };
    let output = TransactionOutput::from((tx, transaction_version, msg_hash));
    let expected_status = TransactionStatus {
        finality_status: TransactionFinalityStatus::AcceptedOnL2,
        execution_status: output.execution_status().clone(),
    };
    let (json_response, res) =
        raw_call::<_, _, TransactionStatus>(&module, method_name, &[transaction_hash]).await;
    assert_eq!(res.unwrap(), expected_status);
    assert!(validate_schema(
        &get_starknet_spec_api_schema_for_method_results(
            &[(
                SpecFile::StarknetApiOpenrpc,
                &[method_name_to_spec_method_name(method_name).as_str()]
            )],
            &VERSION,
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
    let res = module.call::<_, TransactionStatus>(method_name, [transaction_hash]).await.unwrap();
    assert_eq!(res.finality_status, TransactionFinalityStatus::AcceptedOnL1);
    assert_eq!(res.execution_status, TransactionExecutionStatus::Succeeded);

    // Add a pending transaction and ask for its status.
    let mut rng = get_rng();
    let (client_transaction, client_transaction_receipt, expected_receipt) =
        generate_client_transaction_client_receipt_and_rpc_receipt(&mut rng);
    let expected_status = TransactionStatus {
        finality_status: TransactionFinalityStatus::AcceptedOnL2,
        execution_status: expected_receipt.output.execution_status().clone(),
    };

    {
        let pending_block = &mut pending_data.write().await.block;
        pending_block.transactions.push(client_transaction.clone());
        pending_block.transaction_receipts.push(client_transaction_receipt.clone());
    }
    let (json_response, result) = raw_call::<_, _, TransactionStatus>(
        &module,
        method_name,
        &[client_transaction_receipt.transaction_hash],
    )
    .await;
    assert_eq!(result.unwrap(), expected_status);
    // Validating schema again since pending has a different schema
    assert!(validate_schema(
        &get_starknet_spec_api_schema_for_method_results(
            &[(
                SpecFile::StarknetApiOpenrpc,
                &[method_name_to_spec_method_name(method_name).as_str()]
            )],
            &VERSION,
        ),
        &json_response["result"],
    ));

    // Ask for transaction status when the pending block is not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    let (_, res) = raw_call::<_, _, TransactionStatus>(
        &module,
        method_name,
        &[client_transaction_receipt.transaction_hash],
    )
    .await;
    assert_eq!(res.unwrap_err(), TRANSACTION_HASH_NOT_FOUND.into());

    // Ask for an invalid transaction.
    call_api_then_assert_and_validate_schema_for_err::<_, TransactionStatus>(
        &module,
        method_name,
        vec![Box::new(TransactionHash(Felt::from(1_u8)))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &TRANSACTION_HASH_NOT_FOUND.into(),
    )
    .await;
}

#[tokio::test]
async fn get_transaction_receipt() {
    let method_name = "starknet_V0_6_getTransactionReceipt";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .update_starknet_version(&block.header.block_number, &StarknetVersion::default())
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let transaction_hash = block.body.transaction_hashes[0];
    let transaction_version = match block.body.transactions.index(0) {
        StarknetApiTransaction::Declare(tx) => tx.version(),
        StarknetApiTransaction::Deploy(tx) => tx.version,
        StarknetApiTransaction::DeployAccount(tx) => tx.version(),
        StarknetApiTransaction::Invoke(tx) => tx.version(),
        StarknetApiTransaction::L1Handler(tx) => tx.version,
    };
    let tx = block.body.transactions.index(0).clone();
    let msg_hash = match tx {
        starknet_api::transaction::Transaction::L1Handler(tx) => Some(tx.calc_msg_hash()),
        _ => None,
    };
    let output = TransactionOutput::from((
        block.body.transaction_outputs.index(0).clone(),
        transaction_version,
        msg_hash,
    ));
    let expected_receipt = TransactionReceipt {
        finality_status: TransactionFinalityStatus::AcceptedOnL2,
        transaction_hash,
        block_hash: block.header.block_hash,
        block_number: block.header.block_number,
        output,
    };
    let (json_response, res) =
        raw_call::<_, _, TransactionReceipt>(&module, method_name, &[transaction_hash]).await;
    // The returned jsons of some transaction outputs are the same. When deserialized, the first
    // struct in the TransactionOutput enum that matches the json is chosen. To not depend here
    // on the order of structs we compare the serialized data.
    assert_eq!(
        serde_json::to_value(res.unwrap()).unwrap(),
        serde_json::to_value(&expected_receipt).unwrap(),
    );
    assert!(validate_schema(
        &get_starknet_spec_api_schema_for_method_results(
            &[(
                SpecFile::StarknetApiOpenrpc,
                &[method_name_to_spec_method_name(method_name).as_str()]
            )],
            &VERSION,
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
    let res = module.call::<_, TransactionReceipt>(method_name, [transaction_hash]).await.unwrap();
    assert_eq!(res.finality_status, TransactionFinalityStatus::AcceptedOnL1);
    assert_eq!(res.output.execution_status(), &TransactionExecutionStatus::Succeeded);

    // Add a pending transaction and ask for its receipt.
    let mut rng = get_rng();
    let (client_transaction, client_transaction_receipt, expected_receipt) =
        generate_client_transaction_client_receipt_and_rpc_receipt(&mut rng);

    {
        let pending_block = &mut pending_data.write().await.block;
        pending_block.transactions.push(client_transaction.clone());
        pending_block.transaction_receipts.push(client_transaction_receipt.clone());
    }

    let expected_result = GeneralTransactionReceipt::PendingTransactionReceipt(expected_receipt);
    let (json_response, result) = raw_call::<_, _, PendingTransactionReceipt>(
        &module,
        method_name,
        &[client_transaction_receipt.transaction_hash],
    )
    .await;
    // See above for explanation why we compare the json strings.
    assert_eq!(
        serde_json::to_value(result.unwrap()).unwrap(),
        serde_json::to_value(&expected_result).unwrap(),
    );
    // Validating schema again since pending has a different schema
    assert!(validate_schema(
        &get_starknet_spec_api_schema_for_method_results(
            &[(
                SpecFile::StarknetApiOpenrpc,
                &[method_name_to_spec_method_name(method_name).as_str()]
            )],
            &VERSION,
        ),
        &json_response["result"],
    ));

    // Ask for transaction receipt when the pending block is not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    let (_, res) = raw_call::<_, _, TransactionReceipt>(
        &module,
        method_name,
        &[client_transaction_receipt.transaction_hash],
    )
    .await;
    assert_eq!(res.unwrap_err(), TRANSACTION_HASH_NOT_FOUND.into());

    // Ask for an invalid transaction.
    call_api_then_assert_and_validate_schema_for_err::<_, TransactionReceipt>(
        &module,
        method_name,
        vec![Box::new(TransactionHash(Felt::from(1_u8)))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &TRANSACTION_HASH_NOT_FOUND.into(),
    )
    .await;
}

#[tokio::test]
async fn get_class_at() {
    let method_name = "starknet_V0_6_getClassAt";
    let pending_data = get_test_pending_data();
    let pending_classes = get_test_pending_classes();
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer_from_params::<JsonRpcServerImpl>(
            None,
            None,
            Some(pending_data.clone()),
            Some(pending_classes.clone()),
            None,
        );
    let parent_header = BlockHeader::default();
    let header = BlockHeader {
        block_hash: BlockHash(Felt::ONE),
        block_number: BlockNumber(1),
        parent_hash: parent_header.block_hash,
        ..BlockHeader::default()
    };
    let mut diff = get_test_state_diff();
    // Add a deployed contract with Cairo 1 class.
    let new_class_hash = diff.declared_classes.get_index(0).unwrap().0;
    diff.deployed_contracts.insert(ContractAddress(patricia_key!(0x2)), *new_class_hash);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_header.block_number, &parent_header)
        .unwrap()
        .update_starknet_version(&parent_header.block_number, &StarknetVersion::default())
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

    let pending_address: ContractAddress = random::<u64>().into();
    let pending_class_hash = ClassHash(random::<u64>().into());
    let pending_class = ApiContractClass::ContractClass(
        StarknetApiContractClass::get_test_instance(&mut get_rng()),
    );
    pending_data
        .write()
        .await
        .state_update
        .state_diff
        .deployed_contracts
        .push(ClientDeployedContract { address: pending_address, class_hash: pending_class_hash });
    pending_data.write().await.block.parent_block_hash = header.block_hash;
    pending_classes.write().await.add_class(pending_class_hash, pending_class.clone());

    // Deprecated Class
    let (class_hash, contract_class) = diff.deprecated_declared_classes.get_index(0).unwrap();
    let expected_contract_class = contract_class.clone().try_into().unwrap();
    assert_eq!(diff.deployed_contracts.get_index(0).unwrap().1, class_hash);
    let address = diff.deployed_contracts.get_index(0).unwrap().0;

    // Get class by block hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
            Box::new(*address),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
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
    let expected_contract_class = contract_class.clone().into();
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

    // Get class hash of pending block.
    let res = module
        .call::<_, GatewayContractClass>(method_name, (BlockId::Tag(Tag::Pending), pending_address))
        .await
        .unwrap();
    assert_eq!(res, pending_class.try_into().unwrap());

    // Get class hash of pending block when it's not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    call_api_then_assert_and_validate_schema_for_err::<_, ContractClass>(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending)), Box::new(pending_address)],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CONTRACT_NOT_FOUND.into(),
    )
    .await;

    // Invalid Call
    // Ask for an invalid contract.
    call_api_then_assert_and_validate_schema_for_err::<_, DeprecatedContractClass>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
            Box::new(ContractAddress(patricia_key!(0x12))),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CONTRACT_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == CONTRACT_NOT_FOUND.into());

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, DeprecatedContractClass>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
                Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                    .unwrap(),
            )))),
            Box::new(*address),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());
}

#[tokio::test]
async fn get_class_hash_at() {
    let method_name = "starknet_V0_6_getClassHashAt";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let header = BlockHeader::default();
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .update_starknet_version(&header.block_number, &StarknetVersion::default())
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (address, expected_class_hash) = diff.deployed_contracts.get_index(0).unwrap();

    let pending_address: ContractAddress = random::<u64>().into();
    let pending_class_hash = ClassHash(random::<u64>().into());
    pending_data
        .write()
        .await
        .state_update
        .state_diff
        .deployed_contracts
        .push(ClientDeployedContract { address: pending_address, class_hash: pending_class_hash });
    pending_data.write().await.block.parent_block_hash = header.block_hash;

    // Get class hash by block hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
            Box::new(*address),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
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

    // Get class hash by latest tag.
    let res = module
        .call::<_, ClassHash>(method_name, (BlockId::Tag(Tag::Latest), *address))
        .await
        .unwrap();
    assert_eq!(res, *expected_class_hash);

    // Get class hash of pending block.
    let res = module
        .call::<_, ClassHash>(method_name, (BlockId::Tag(Tag::Pending), *address))
        .await
        .unwrap();
    assert_eq!(res, *expected_class_hash);

    let res = module
        .call::<_, ClassHash>(method_name, (BlockId::Tag(Tag::Pending), pending_address))
        .await
        .unwrap();
    assert_eq!(res, pending_class_hash);

    // Get class hash of pending block when it's replaced.
    let replaced_class_hash = ClassHash(random::<u64>().into());
    pending_data.write().await.state_update.state_diff.replaced_classes.append(&mut vec![
        ClientReplacedClass { address: *address, class_hash: replaced_class_hash },
        ClientReplacedClass { address: pending_address, class_hash: replaced_class_hash },
    ]);

    let res = module
        .call::<_, ClassHash>(method_name, (BlockId::Tag(Tag::Pending), *address))
        .await
        .unwrap();
    assert_eq!(res, replaced_class_hash);

    let res = module
        .call::<_, ClassHash>(method_name, (BlockId::Tag(Tag::Pending), pending_address))
        .await
        .unwrap();
    assert_eq!(res, replaced_class_hash);

    // Get class hash of pending block when it's not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    call_api_then_assert_and_validate_schema_for_err::<_, ClassHash>(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending)), Box::new(pending_address)],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CONTRACT_NOT_FOUND.into(),
    )
    .await;

    let res = module
        .call::<_, ClassHash>(method_name, (BlockId::Tag(Tag::Pending), *address))
        .await
        .unwrap();
    assert_eq!(res, *expected_class_hash);

    // Ask for an invalid contract.
    call_api_then_assert_and_validate_schema_for_err::<_, ClassHash>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
            Box::new(ContractAddress(patricia_key!(0x12))),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CONTRACT_NOT_FOUND.into(),
    )
    .await;

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, ClassHash>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
                Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                    .unwrap(),
            )))),
            Box::new(*address),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());
}

#[tokio::test]
async fn get_nonce() {
    let method_name = "starknet_V0_6_getNonce";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let header = BlockHeader::default();
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .update_starknet_version(&header.block_number, &StarknetVersion::default())
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (address, expected_nonce) = diff.nonces.get_index(0).unwrap();

    // Get nonce by block hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
            Box::new(*address),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        expected_nonce,
    )
    .await;

    // Get nonce by block number.
    let res = module
        .call::<_, Nonce>(
            method_name,
            (BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)), *address),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_nonce);

    // Ask for nonce in pending block when it wasn't changed in pending block.
    let res =
        module.call::<_, Nonce>(method_name, (BlockId::Tag(Tag::Pending), *address)).await.unwrap();
    assert_eq!(res, *expected_nonce);

    // Ask for nonce in pending block when it was changed in pending block.
    let new_nonce = Nonce(Felt::from(1234_u128));
    pending_data.write().await.state_update.state_diff.nonces.insert(*address, new_nonce);
    let res =
        module.call::<_, Nonce>(method_name, (BlockId::Tag(Tag::Pending), *address)).await.unwrap();
    assert_eq!(res, new_nonce);

    // Ask for nonce in pending block where the contract is deployed in the pending block.
    let new_pending_contract_address = ContractAddress(patricia_key!(0x1234));
    pending_data
        .write()
        .await
        .state_update
        .state_diff
        .nonces
        .insert(new_pending_contract_address, new_nonce);
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending)), Box::new(new_pending_contract_address)],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &new_nonce,
    )
    .await;

    // Ask for nonce in pending block when the pending block is not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    let res =
        module.call::<_, Nonce>(method_name, (BlockId::Tag(Tag::Pending), *address)).await.unwrap();
    assert_eq!(res, *expected_nonce);

    // Ask for nonce in pending block where the contract is deployed in the pending block, and the
    // pending block is not up to date.
    // Expected outcome: Failure due to contract not found.
    call_api_then_assert_and_validate_schema_for_err::<_, Felt>(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending)), Box::new(new_pending_contract_address)],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CONTRACT_NOT_FOUND.into(),
    )
    .await;

    // Ask for an invalid contract.
    call_api_then_assert_and_validate_schema_for_err::<_, Nonce>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
            Box::new(ContractAddress(patricia_key!(0x31))),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CONTRACT_NOT_FOUND.into(),
    )
    .await;

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, Nonce>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
                Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                    .unwrap(),
            )))),
            Box::new(*address),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());
}

#[tokio::test]
async fn get_storage_at() {
    let method_name = "starknet_V0_6_getStorageAt";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let header = BlockHeader::default();
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_number, &header)
        .unwrap()
        .update_starknet_version(&header.block_number, &StarknetVersion::default())
        .unwrap()
        .append_state_diff(header.block_number, diff.clone(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (address, storage_entries) = diff.storage_diffs.get_index(0).unwrap();
    let (key, expected_value) = storage_entries.get_index(0).unwrap();

    // Get storage by block hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![
            Box::new(*address),
            Box::new(*key),
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        expected_value,
    )
    .await;

    // Get storage by block number.
    let res = module
        .call::<_, Felt>(
            method_name,
            (*address, *key, BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number))),
        )
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Ask for storage in pending block when contract's storage wasn't changed in pending block.
    let res = module
        .call::<_, Felt>(method_name, (*address, key, BlockId::Tag(Tag::Pending)))
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Ask for storage in pending block when it wasn't changed in pending block.
    let other_key = random::<u64>().into();
    let other_value = random::<u64>().into();
    pending_data
        .write()
        .await
        .state_update
        .state_diff
        .storage_diffs
        .insert(*address, vec![ClientStorageEntry { key: other_key, value: other_value }]);
    let res = module
        .call::<_, Felt>(method_name, (*address, key, BlockId::Tag(Tag::Pending)))
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Ask for storage in pending block when it was changed in pending block.
    let res = module
        .call::<_, Felt>(method_name, (*address, other_key, BlockId::Tag(Tag::Pending)))
        .await
        .unwrap();
    assert_eq!(res, other_value);

    // Ask for storage that was changed both in the pending block and the non-pending block.
    pending_data
        .write()
        .await
        .state_update
        .state_diff
        .storage_diffs
        .insert(*address, vec![ClientStorageEntry { key: *key, value: other_value }]);
    let res = module
        .call::<_, Felt>(method_name, (*address, key, BlockId::Tag(Tag::Pending)))
        .await
        .unwrap();
    assert_eq!(res, other_value);

    // Ask for storage in pending block when the pending block is not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    let res = module
        .call::<_, Felt>(method_name, (*address, other_key, BlockId::Tag(Tag::Pending)))
        .await
        .unwrap();
    assert_eq!(res, Felt::default());

    // Ask for storage updated both in pending block and non-pending block when the pending block is
    // not up to date.
    let res = module
        .call::<_, Felt>(method_name, (*address, *key, BlockId::Tag(Tag::Pending)))
        .await
        .unwrap();
    assert_eq!(res, *expected_value);

    // Ask for storage in pending block where the contract is deployed in the pending block, and the
    // pending block is not up to date.
    // Expected outcome: Failure due to contract not found.
    let key = StorageKey(patricia_key!(0x1001));
    let contract_address = ContractAddress(patricia_key!(0x1234));
    pending_data
        .write()
        .await
        .state_update
        .state_diff
        .storage_diffs
        .insert(contract_address, vec![ClientStorageEntry { key, value: Felt::default() }]);
    call_api_then_assert_and_validate_schema_for_err::<_, Felt>(
        &module,
        method_name,
        vec![Box::new(contract_address), Box::new(key), Box::new(BlockId::Tag(Tag::Pending))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CONTRACT_NOT_FOUND.into(),
    )
    .await;

    // Ask for storage at address 0x1 - the block hash table contract address
    let res = module
        .call::<_, Felt>(
            "starknet_V0_6_getStorageAt",
            (
                *BLOCK_HASH_TABLE_ADDRESS,
                key,
                BlockId::HashOrNumber(BlockHashOrNumber::Number(header.block_number)),
            ),
        )
        .await
        .unwrap();
    assert_eq!(res, Felt::default());

    // Ask for an invalid contract.
    call_api_then_assert_and_validate_schema_for_err::<_, Felt>(
        &module,
        method_name,
        vec![
            Box::new(ContractAddress(patricia_key!(0x12))),
            Box::new(key),
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash))),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &CONTRACT_NOT_FOUND.into(),
    )
    .await;

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, Felt>(
        &module,
        method_name,
        vec![
            Box::new(*address),
            Box::new(key),
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
                Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                    .unwrap(),
            )))),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
    )
    .await;

    // Ask for an invalid block number.
    let err = module
        .call::<_, Felt>(
            method_name,
            (*address, key, BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))),
        )
        .await
        .unwrap_err();
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());
}

fn generate_client_transaction_client_receipt_and_rpc_receipt(
    rng: &mut ChaCha8Rng,
) -> (ClientTransaction, ClientTransactionReceipt, PendingTransactionReceipt) {
    let pending_transaction_hash = TransactionHash(Felt::from(rng.next_u64()));
    let mut client_transaction_receipt = ClientTransactionReceipt::get_test_instance(rng);
    client_transaction_receipt.transaction_hash = pending_transaction_hash;
    client_transaction_receipt.execution_resources.n_memory_holes = 1;
    client_transaction_receipt.execution_resources.n_steps = 1;
    client_transaction_receipt.execution_resources.builtin_instance_counter.retain(|_, v| *v > 0);
    // Generating a transaction until we receive a transaction that can have pending output (i.e a
    // non-deploy transaction).
    let (mut client_transaction, output) = loop {
        let (client_transaction, _) = generate_client_transaction_and_rpc_transaction(rng);
        let starknet_api_output = client_transaction_receipt
            .clone()
            .into_starknet_api_transaction_output(&client_transaction);
        let msg_hash = match &client_transaction {
            starknet_client::reader::objects::transaction::Transaction::L1Handler(tx) => {
                Some(tx.calc_msg_hash())
            }
            _ => None,
        };
        let maybe_output = PendingTransactionOutput::try_from(TransactionOutput::from((
            starknet_api_output,
            client_transaction.transaction_version(),
            msg_hash,
        )));
        let Ok(output) = maybe_output else {
            continue;
        };
        break (client_transaction, output);
    };
    *client_transaction.transaction_hash_mut() = pending_transaction_hash;
    (
        client_transaction,
        client_transaction_receipt,
        PendingTransactionReceipt {
            finality_status: PendingTransactionFinalityStatus::AcceptedOnL2,
            transaction_hash: pending_transaction_hash,
            output,
        },
    )
}

fn generate_client_transaction_and_rpc_transaction(
    rng: &mut ChaCha8Rng,
) -> (ClientTransaction, TransactionWithHash) {
    // TODO(shahak): Remove retry once v3 transactions are supported and the impl of TryInto will
    // become impl of Into.
    loop {
        let client_transaction = ClientTransaction::get_test_instance(rng);
        let Ok(starknet_api_transaction): Result<StarknetApiTransaction, _> =
            client_transaction.clone().try_into()
        else {
            continue;
        };
        let Ok(rpc_transaction) = starknet_api_transaction.try_into() else {
            continue;
        };
        let transaction_hash = client_transaction.transaction_hash();
        break (
            client_transaction,
            TransactionWithHash { transaction: rpc_transaction, transaction_hash },
        );
    }
}

#[tokio::test]
async fn get_transaction_by_hash() {
    let method_name = "starknet_V0_6_getTransactionByHash";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let mut block = get_test_block(1, None, None, None);
    // Change the transaction hash from 0 to a random value, so that later on we can add a
    // transaction with 0 hash to the pending block.
    block.body.transaction_hashes[0] = TransactionHash(Felt::from(random::<u64>()));
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = TransactionWithHash {
        transaction: block.body.transactions[0].clone().try_into().unwrap(),
        transaction_hash: block.body.transaction_hashes[0],
    };
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(block.body.transaction_hashes[0])],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_transaction,
    )
    .await;

    // Ask for a transaction in the pending block.
    let (client_transaction, expected_transaction_with_hash) =
        generate_client_transaction_and_rpc_transaction(&mut get_rng());
    pending_data.write().await.block.transactions.push(client_transaction.clone());
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(client_transaction.transaction_hash())],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_transaction_with_hash,
    )
    .await;

    // Get pending block when it's not updated.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    call_api_then_assert_and_validate_schema_for_err::<_, TransactionWithHash>(
        &module,
        method_name,
        vec![Box::new(client_transaction.transaction_hash())],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &TRANSACTION_HASH_NOT_FOUND.into(),
    )
    .await;

    // Ask for an invalid transaction.
    call_api_then_assert_and_validate_schema_for_err::<_, TransactionWithHash>(
        &module,
        method_name,
        vec![Box::new(TransactionHash(Felt::from(1_u8)))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &TRANSACTION_HASH_NOT_FOUND.into(),
    )
    .await;
}

#[tokio::test]
async fn get_transaction_by_hash_state_only() {
    let method_name = "starknet_V0_6_getTransactionByHash";
    let params = [TransactionHash(Felt::from(1_u8))];
    let (module, _) = get_test_rpc_server_and_storage_writer_from_params::<JsonRpcServerImpl>(
        None,
        None,
        None,
        None,
        Some(StorageScope::StateOnly),
    );

    let (_, err) = raw_call::<_, _, TransactionWithHash>(&module, method_name, &params).await;
    assert_eq!(
        err.unwrap_err(),
        internal_server_error_with_msg("Unsupported method in state-only scope.")
    );
}

#[tokio::test]
async fn get_transaction_by_block_id_and_index() {
    let method_name = "starknet_V0_6_getTransactionByBlockIdAndIndex";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let block = get_test_block(1, None, None, None);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .update_starknet_version(&block.header.block_number, &StarknetVersion::default())
        .unwrap()
        .append_body(block.header.block_number, block.body.clone())
        .unwrap()
        .append_state_diff(
            block.header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
        .unwrap()
        .commit()
        .unwrap();

    let expected_transaction = TransactionWithHash {
        transaction: block.body.transactions[0].clone().try_into().unwrap(),
        transaction_hash: block.body.transaction_hashes[0],
    };

    // Get transaction by block hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash))),
            Box::new(TransactionOffsetInBlock(0)),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
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

    // Get transaction of pending block.
    let (client_transaction, expected_transaction_with_hash) =
        generate_client_transaction_and_rpc_transaction(&mut get_rng());
    pending_data.write().await.block.transactions.push(client_transaction);
    let res = module
        .call::<_, TransactionWithHash>(method_name, (BlockId::Tag(Tag::Pending), 0))
        .await
        .unwrap();
    assert_eq!(res, expected_transaction_with_hash);

    // Ask for an invalid transaction index in pending block.
    call_api_then_assert_and_validate_schema_for_err::<_, TransactionWithHash>(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending)), Box::new(TransactionOffsetInBlock(1))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &INVALID_TRANSACTION_INDEX.into(),
    )
    .await;

    // Get transaction of pending block when the pending block is not up to date.
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());

    call_api_then_assert_and_validate_schema_for_err::<_, TransactionWithHash>(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending)), Box::new(TransactionOffsetInBlock(0))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &INVALID_TRANSACTION_INDEX.into(),
    )
    .await;

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, TransactionWithHash>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
                Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                    .unwrap(),
            )))),
            Box::new(TransactionOffsetInBlock(0)),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());

    // Ask for an invalid transaction index.
    call_api_then_assert_and_validate_schema_for_err::<_, TransactionWithHash>(
        &module,
        method_name,
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(block.header.block_hash))),
            Box::new(TransactionOffsetInBlock(1)),
        ],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &INVALID_TRANSACTION_INDEX.into(),
    )
    .await;
}

#[tokio::test]
async fn get_state_update() {
    let method_name = "starknet_V0_6_getStateUpdate";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let parent_header = BlockHeader::default();
    let expected_pending_old_root = GlobalRoot(Felt::from_hex_unchecked("0x1234"));
    let header = BlockHeader {
        block_hash: BlockHash(Felt::ONE),
        block_number: BlockNumber(1),
        parent_hash: parent_header.block_hash,
        state_root: expected_pending_old_root,
        ..BlockHeader::default()
    };
    let diff = get_test_state_diff();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_header.block_number, &parent_header)
        .unwrap()
        .update_starknet_version(&parent_header.block_number, &StarknetVersion::default())
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

    let expected_old_root = parent_header.state_root;
    let expected_state_diff = ThinStateDiff::from(starknet_api::state::ThinStateDiff::from(diff));
    let expected_update = StateUpdate::AcceptedStateUpdate(AcceptedStateUpdate {
        block_hash: header.block_hash,
        new_root: header.state_root,
        old_root: expected_old_root,
        state_diff: expected_state_diff.clone(),
    });

    // Get state update by block hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
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

    // Get state update of pending block.
    let expected_pending_update = StateUpdate::PendingStateUpdate(PendingStateUpdate {
        old_root: expected_old_root,
        state_diff: expected_state_diff.clone(),
    });
    pending_data.write().await.block.parent_block_hash = header.block_hash;
    pending_data.write().await.state_update = ClientPendingStateUpdate {
        old_root: expected_old_root,
        state_diff: ClientStateDiff {
            storage_diffs: IndexMap::from_iter(expected_state_diff.storage_diffs.into_iter().map(
                |StorageDiff { address, storage_entries }| {
                    let storage_entries = Vec::from_iter(
                        storage_entries
                            .into_iter()
                            .map(|StorageEntry { key, value }| ClientStorageEntry { key, value }),
                    );
                    (address, storage_entries)
                },
            )),
            deployed_contracts: Vec::from_iter(
                expected_state_diff.deployed_contracts.into_iter().map(
                    |DeployedContract { address, class_hash }| ClientDeployedContract {
                        address,
                        class_hash,
                    },
                ),
            ),
            declared_classes: expected_state_diff
                .declared_classes
                .into_iter()
                .map(|ClassHashes { class_hash, compiled_class_hash }| {
                    ClientDeclaredClassHashEntry { class_hash, compiled_class_hash }
                })
                .collect(),
            old_declared_contracts: expected_state_diff.deprecated_declared_classes,
            nonces: IndexMap::from_iter(
                expected_state_diff
                    .nonces
                    .into_iter()
                    .map(|ContractNonce { contract_address, nonce }| (contract_address, nonce)),
            ),
            replaced_classes: Vec::from_iter(expected_state_diff.replaced_classes.into_iter().map(
                |ReplacedClasses { contract_address, class_hash }| ClientReplacedClass {
                    address: contract_address,
                    class_hash,
                },
            )),
        },
    };
    // Validating schema because the state diff of pending block contains less fields.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::Tag(Tag::Pending))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_pending_update,
    )
    .await;

    // Get state update of pending block when the pending block is not up to date.
    let expected_pending_update = StateUpdate::PendingStateUpdate(PendingStateUpdate {
        old_root: expected_pending_old_root,
        ..PendingStateUpdate::default()
    });
    pending_data.write().await.block.parent_block_hash = BlockHash(random::<u64>().into());
    let res =
        module.call::<_, StateUpdate>(method_name, [BlockId::Tag(Tag::Pending)]).await.unwrap();
    assert_eq!(res, expected_pending_update);

    // Ask for an invalid block hash.
    call_api_then_assert_and_validate_schema_for_err::<_, StateUpdate>(
        &module,
        method_name,
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Hash(BlockHash(
            Felt::from_hex("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5484")
                .unwrap(),
        ))))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &BLOCK_NOT_FOUND.into(),
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
    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());
}

#[tokio::test]
async fn get_state_update_with_empty_storage_diff() {
    let method_name = "starknet_V0_6_getStateUpdate";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let state_diff = starknet_api::state::StateDiff {
        storage_diffs: indexmap!(ContractAddress::default() => indexmap![]),
        ..Default::default()
    };
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .update_starknet_version(&BlockNumber(0), &StarknetVersion::default())
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_state_diff(BlockNumber(0), state_diff, IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    // The empty storage diff should be removed in the result.
    let expected_state_diff =
        ThinStateDiff::from(starknet_api::state::ThinStateDiff::from(StateDiff::default()));
    let expected_update = StateUpdate::AcceptedStateUpdate(AcceptedStateUpdate {
        state_diff: expected_state_diff.clone(),
        ..Default::default()
    });

    // Get state update by block hash.
    call_api_then_assert_and_validate_schema_for_result(
        &module,
        method_name,
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))))],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &expected_update,
    )
    .await;
}

#[derive(Clone)]
struct EventMetadata {
    pub address: Option<ContractAddress>,
    pub keys: Option<Vec<EventKey>>,
}

const DEFAULT_EVENT_METADATA: EventMetadata = EventMetadata { address: None, keys: None };

impl EventMetadata {
    pub fn generate_event(&self, rng: &mut ChaCha8Rng) -> StarknetApiEvent {
        StarknetApiEvent {
            from_address: self.address.unwrap_or_else(|| ContractAddress(rng.next_u64().into())),
            content: EventContent {
                keys: self.keys.clone().unwrap_or_else(|| vec![EventKey(rng.next_u64().into())]),
                data: EventData(vec![rng.next_u64().into()]),
            },
        }
    }
}

#[derive(Clone, Default)]
struct BlockMetadata(pub Vec<Vec<EventMetadata>>);

impl BlockMetadata {
    pub fn generate_block(
        &self,
        rng: &mut ChaCha8Rng,
        parent_hash: BlockHash,
        block_number: BlockNumber,
    ) -> StarknetApiBlock {
        // Generate a block with no events, And then add the events manually.
        let mut block = get_test_block(self.0.len(), Some(0), None, None);
        block.header.parent_hash = parent_hash;
        block.header.block_number = block_number;
        block.header.block_hash = BlockHash(rng.next_u64().into());
        // Randomize the transaction hashes because get_test_block returns constant hashes
        for transaction_hash in &mut block.body.transaction_hashes {
            *transaction_hash = TransactionHash(rng.next_u64().into());
        }

        for (output, event_metadatas_of_tx) in
            block.body.transaction_outputs.iter_mut().zip(self.0.iter())
        {
            let events = match output {
                StarknetApiTransactionOutput::Declare(transaction) => &mut transaction.events,
                StarknetApiTransactionOutput::Deploy(transaction) => &mut transaction.events,
                StarknetApiTransactionOutput::DeployAccount(transaction) => &mut transaction.events,
                StarknetApiTransactionOutput::Invoke(transaction) => &mut transaction.events,
                StarknetApiTransactionOutput::L1Handler(transaction) => &mut transaction.events,
            };
            for event_metadata in event_metadatas_of_tx {
                events.push(event_metadata.generate_event(rng));
            }
        }
        block
    }

    pub fn generate_pending_block(
        &self,
        rng: &mut ChaCha8Rng,
        parent_hash: BlockHash,
    ) -> PendingBlock {
        let transaction_hashes = iter::repeat_with(|| TransactionHash(rng.next_u64().into()))
            .take(self.0.len())
            .collect::<Vec<_>>();
        PendingBlock {
            parent_block_hash: parent_hash,
            transactions: transaction_hashes
                .iter()
                .map(|transaction_hash| {
                    let mut transaction = ClientTransaction::get_test_instance(rng);
                    *transaction.transaction_hash_mut() = *transaction_hash;
                    transaction
                })
                .collect(),
            transaction_receipts: transaction_hashes
                .iter()
                .zip(self.0.iter())
                .enumerate()
                .map(|(i, (transaction_hash, event_metadatas_of_tx))| ClientTransactionReceipt {
                    transaction_index: TransactionOffsetInBlock(i),
                    transaction_hash: *transaction_hash,
                    events: event_metadatas_of_tx
                        .iter()
                        .map(|event_metadata| event_metadata.generate_event(rng))
                        .collect(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }
}

async fn test_get_events(
    block_metadatas: Vec<BlockMetadata>,
    pending_block_metadata: Option<BlockMetadata>,
    is_pending_up_to_date: bool,
    mut filter: EventFilter,
    expected_result_by_index: Vec<(Vec<EventIndex>, Option<ContinuationTokenAsStruct>)>,
) {
    let method_name = "starknet_V0_6_getEvents";
    let pending_data = get_test_pending_data();
    let (module, mut storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerImpl,
    >(None, None, Some(pending_data.clone()), None, None);
    let mut rng = get_rng();

    let mut event_index_to_event = HashMap::<EventIndex, Event>::new();
    let mut parent_hash = BlockHash(Felt::from(GENESIS_HASH));
    let mut rw_txn = storage_writer.begin_rw_txn().unwrap();
    for (i, block_metadata) in block_metadatas.iter().enumerate() {
        let block_number = BlockNumber(i as u64);
        let block = block_metadata.generate_block(&mut rng, parent_hash, block_number);

        parent_hash = block.header.block_hash;

        for (i_transaction, (output, transaction_hash)) in block
            .body
            .transaction_outputs
            .iter()
            .zip(block.body.transaction_hashes.iter().cloned())
            .enumerate()
        {
            for (i_event, event) in output.events().iter().cloned().enumerate() {
                event_index_to_event.insert(
                    EventIndex(
                        TransactionIndex(block_number, TransactionOffsetInBlock(i_transaction)),
                        EventIndexInTransactionOutput(i_event),
                    ),
                    Event {
                        block_hash: Some(block.header.block_hash),
                        block_number: Some(block_number),
                        transaction_hash,
                        event,
                    },
                );
            }
        }

        rw_txn = rw_txn
            .append_header(block_number, &block.header)
            .unwrap()
            .update_starknet_version(&block_number, &StarknetVersion::default())
            .unwrap()
            .append_body(block_number, block.body)
            .unwrap()
            .append_state_diff(
                block.header.block_number,
                starknet_api::state::StateDiff::default(),
                IndexMap::new(),
            )
            .unwrap();
    }
    rw_txn.commit().unwrap();

    if let Some(pending_block_metadata) = pending_block_metadata {
        if !is_pending_up_to_date {
            parent_hash = BlockHash(rng.next_u64().into());
        }
        let pending_block = pending_block_metadata.generate_pending_block(&mut rng, parent_hash);

        for (i_transaction, receipt) in pending_block.transaction_receipts.iter().enumerate() {
            for (i_event, event) in receipt.events.iter().cloned().enumerate() {
                event_index_to_event.insert(
                    EventIndex(
                        TransactionIndex(
                            BlockNumber(block_metadatas.len() as u64),
                            TransactionOffsetInBlock(i_transaction),
                        ),
                        EventIndexInTransactionOutput(i_event),
                    ),
                    Event {
                        block_hash: None,
                        block_number: None,
                        transaction_hash: receipt.transaction_hash,
                        event,
                    },
                );
            }
        }

        pending_data.write().await.block = pending_block;
    }

    for (expected_event_indices, expected_continuation_token) in expected_result_by_index {
        let expected_result = EventsChunk {
            events: expected_event_indices
                .iter()
                .map(|event_index| event_index_to_event.get(event_index).unwrap())
                .cloned()
                .collect(),
            continuation_token: expected_continuation_token
                .map(|x| ContinuationToken::new(x).unwrap()),
        };
        call_api_then_assert_and_validate_schema_for_result(
            &module,
            method_name,
            vec![Box::new(filter.clone())],
            &VERSION,
            SpecFile::StarknetApiOpenrpc,
            &expected_result,
        )
        .await;
        filter.continuation_token = expected_result.continuation_token;
    }
}

lazy_static! {
    static ref BLOCKS_METADATA_FOR_CHUNK_ACROSS_2_BLOCKS_TEST: Vec<BlockMetadata> = vec![
        BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA], vec![DEFAULT_EVENT_METADATA]]),
        // There should be a chunk that starts at the non-first transaction of the second block, in
        // order to test the continuation token for pending.
        BlockMetadata(vec![
            [DEFAULT_EVENT_METADATA; 3].to_vec(),
            [DEFAULT_EVENT_METADATA; 2].to_vec(),
        ]),
    ];
    static ref EVENT_FILTER_FOR_CHUNK_ACROSS_2_BLOCKS_TEST: EventFilter =
        EventFilter { chunk_size: 3, ..Default::default() };
    static ref EXPECTED_RESULT_BY_INDEX_FOR_CHUNK_ACROSS_2_BLOCKS_TEST: Vec<(Vec<EventIndex>, Option<ContinuationTokenAsStruct>,)> = vec![
        (
            vec![
                EventIndex(
                    TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)),
                    EventIndexInTransactionOutput(0),
                ),
                EventIndex(
                    TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(1)),
                    EventIndexInTransactionOutput(0),
                ),
                EventIndex(
                    TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
                    EventIndexInTransactionOutput(0),
                ),
            ],
            Some(ContinuationTokenAsStruct(EventIndex(
                TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
                EventIndexInTransactionOutput(1),
            ))),
        ),
        (
            vec![
                EventIndex(
                    TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
                    EventIndexInTransactionOutput(1),
                ),
                EventIndex(
                    TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
                    EventIndexInTransactionOutput(2),
                ),
                EventIndex(
                    TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(1)),
                    EventIndexInTransactionOutput(0),
                ),
            ],
            Some(ContinuationTokenAsStruct(EventIndex(
                TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(1)),
                EventIndexInTransactionOutput(1),
            ))),
        ),
        (
            vec![EventIndex(
                TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(1)),
                EventIndexInTransactionOutput(1),
            )],
            None,
        ),
    ];
}

#[tokio::test]
async fn get_events_chunk_across_2_blocks() {
    let pending_block_metadata = None;
    let is_pending_up_to_date = true;
    test_get_events(
        BLOCKS_METADATA_FOR_CHUNK_ACROSS_2_BLOCKS_TEST.clone(),
        pending_block_metadata,
        is_pending_up_to_date,
        EVENT_FILTER_FOR_CHUNK_ACROSS_2_BLOCKS_TEST.clone(),
        EXPECTED_RESULT_BY_INDEX_FOR_CHUNK_ACROSS_2_BLOCKS_TEST.clone(),
    )
    .await;
}

#[tokio::test]
async fn get_events_chunk_across_block_and_pending_block() {
    let mut blocks_metadata = BLOCKS_METADATA_FOR_CHUNK_ACROSS_2_BLOCKS_TEST.clone();
    let pending_block_metadata = Some(blocks_metadata.pop().unwrap());
    let is_pending_up_to_date = true;
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EVENT_FILTER_FOR_CHUNK_ACROSS_2_BLOCKS_TEST.clone(),
        EXPECTED_RESULT_BY_INDEX_FOR_CHUNK_ACROSS_2_BLOCKS_TEST.clone(),
    )
    .await;
}

#[tokio::test]
async fn get_events_address_filter() {
    let address = ContractAddress(patricia_key!(0x22));
    let blocks_metadata = vec![BlockMetadata(vec![vec![
        DEFAULT_EVENT_METADATA,
        EventMetadata { address: Some(address), keys: None },
        DEFAULT_EVENT_METADATA,
    ]])];
    let pending_block_metadata = None;
    let is_pending_up_to_date = true;
    let expected_result_by_index = vec![(
        vec![EventIndex(
            TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(1),
        )],
        None,
    )];
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EventFilter { chunk_size: 2, address: Some(address), ..Default::default() },
        expected_result_by_index,
    )
    .await;
}

#[tokio::test]
async fn get_events_pending_address_filter() {
    let address = ContractAddress(patricia_key!(0x22));
    // As a special edge case, the function get_events doesn't return events if there are no
    // accepted blocks, even if there is a pending block. Therefore, we need to have a block in the
    // storage.
    let blocks_metadata = vec![BlockMetadata(vec![])];
    let pending_block_metadata = Some(BlockMetadata(vec![vec![
        DEFAULT_EVENT_METADATA,
        EventMetadata { address: Some(address), keys: None },
        DEFAULT_EVENT_METADATA,
    ]]));
    let is_pending_up_to_date = true;
    let expected_result_by_index = vec![(
        vec![EventIndex(
            TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(1),
        )],
        None,
    )];
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EventFilter { chunk_size: 2, address: Some(address), ..Default::default() },
        expected_result_by_index,
    )
    .await;
}

lazy_static! {
    static ref KEY0_0: EventKey = EventKey(Felt::ZERO);
    static ref KEY0_1: EventKey = EventKey(Felt::ONE);
    static ref KEY2_0: EventKey = EventKey(Felt::from_hex_unchecked("0x20"));
    static ref KEY2_1: EventKey = EventKey(Felt::from_hex_unchecked("0x21"));
    static ref UNRELATED_KEY: EventKey = EventKey(Felt::from_hex_unchecked("0xff"));
    static ref BLOCKS_METADATA_FOR_KEYS_FILTER_TEST: Vec<BlockMetadata> =
        // Adding an empty block at the start so that in the pending test there will be an accepted
        // block. See above for explanation on the special edge case of no accepted blocks.
        vec![
            BlockMetadata(vec![]),
            BlockMetadata(vec![vec![
                DEFAULT_EVENT_METADATA,
                EventMetadata {
                    address: None,
                    keys: Some(vec![KEY0_0.clone(), UNRELATED_KEY.clone(), KEY2_1.clone()]),
                },
                EventMetadata {
                    address: None,
                    keys: Some(vec![KEY2_0.clone(), UNRELATED_KEY.clone(), KEY2_1.clone()]),
                },
                EventMetadata {
                    address: None,
                    keys: Some(vec![KEY0_1.clone(), UNRELATED_KEY.clone(), KEY0_0.clone()]),
                },
                EventMetadata {
                    address: None,
                    keys: Some(vec![
                        KEY0_1.clone(),
                        UNRELATED_KEY.clone(),
                        KEY2_0.clone(),
                        UNRELATED_KEY.clone(),
                    ]),
                },
                EventMetadata { address: None, keys: Some(vec![KEY0_1.clone()]) },
                EventMetadata { address: None, keys: Some(vec![]) },
            ]]
        )];
    static ref EVENT_FILTER_FOR_KEYS_FILTER_TEST: EventFilter = EventFilter {
        chunk_size: 6,
        keys: vec![
            HashSet::from([KEY0_0.clone(), KEY0_1.clone()]),
            HashSet::from([]),
            HashSet::from([KEY2_0.clone(), KEY2_1.clone()]),
        ],
        ..Default::default()
    };
    static ref EXPECTED_RESULT_BY_INDEX_FOR_KEYS_FILTER_TEST: Vec<(Vec<EventIndex>, Option<ContinuationTokenAsStruct>,)> =
        vec![(
            vec![
                EventIndex(
                    TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
                    EventIndexInTransactionOutput(1),
                ),
                EventIndex(
                    TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
                    EventIndexInTransactionOutput(4),
                ),
            ],
            None,
        )];
}

#[tokio::test]
async fn get_events_keys_filter() {
    let pending_block_metadata = None;
    let is_pending_up_to_date = true;
    test_get_events(
        BLOCKS_METADATA_FOR_KEYS_FILTER_TEST.clone(),
        pending_block_metadata,
        is_pending_up_to_date,
        EVENT_FILTER_FOR_KEYS_FILTER_TEST.clone(),
        EXPECTED_RESULT_BY_INDEX_FOR_KEYS_FILTER_TEST.clone(),
    )
    .await;
}

#[tokio::test]
async fn get_events_pending_keys_filter() {
    let mut blocks_metadata = BLOCKS_METADATA_FOR_KEYS_FILTER_TEST.clone();
    let pending_block_metadata = Some(blocks_metadata.pop().unwrap());
    let is_pending_up_to_date = true;
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EVENT_FILTER_FOR_KEYS_FILTER_TEST.clone(),
        EXPECTED_RESULT_BY_INDEX_FOR_KEYS_FILTER_TEST.clone(),
    )
    .await;
}

#[tokio::test]
async fn get_events_from_block() {
    let blocks_metadata = vec![
        BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]]),
        BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]]),
    ];
    let pending_block_metadata = None;
    let is_pending_up_to_date = true;
    let expected_result_by_index = vec![(
        vec![EventIndex(
            TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(0),
        )],
        None,
    )];
    test_get_events(
        blocks_metadata.clone(),
        pending_block_metadata.clone(),
        is_pending_up_to_date,
        EventFilter {
            chunk_size: 2,
            from_block: Some(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))),
            ..Default::default()
        },
        expected_result_by_index.clone(),
    )
    .await;
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EventFilter {
            chunk_size: 2,
            from_block: Some(BlockId::Tag(Tag::Latest)),
            ..Default::default()
        },
        expected_result_by_index,
    )
    .await;
}

#[tokio::test]
async fn get_events_from_pending() {
    let blocks_metadata = vec![BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]])];
    let pending_block_metadata = Some(BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]]));
    let is_pending_up_to_date = true;
    let expected_result_by_index = vec![(
        vec![EventIndex(
            TransactionIndex(BlockNumber(1), TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(0),
        )],
        None,
    )];
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EventFilter {
            chunk_size: 2,
            from_block: Some(BlockId::Tag(Tag::Pending)),
            ..Default::default()
        },
        expected_result_by_index,
    )
    .await;
}

#[tokio::test]
async fn get_events_to_block() {
    let blocks_metadata = vec![
        BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]]),
        BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]]),
    ];
    let pending_block_metadata = None;
    let is_pending_up_to_date = true;
    let expected_result_by_index = vec![(
        vec![EventIndex(
            TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(0),
        )],
        None,
    )];
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EventFilter {
            chunk_size: 2,
            to_block: Some(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
            ..Default::default()
        },
        expected_result_by_index,
    )
    .await;
}

// TODO(nevo): add a test that returns the block not found error for getEvents
#[tokio::test]
async fn get_events_no_blocks() {
    let blocks_metadata = vec![BlockMetadata::default()];
    let pending_block_metadata = None;
    let is_pending_up_to_date = true;
    let expected_result_by_index = vec![(vec![], None)];
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EventFilter { chunk_size: 2, ..Default::default() },
        expected_result_by_index,
    )
    .await;
}

#[tokio::test]
async fn get_events_no_blocks_in_filter() {
    let blocks_metadata = vec![
        BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]]),
        BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]]),
    ];
    let pending_block_metadata = None;
    let is_pending_up_to_date = true;
    let expected_result_by_index = vec![(vec![], None)];
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EventFilter {
            chunk_size: 2,
            from_block: Some(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))),
            to_block: Some(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
            ..Default::default()
        },
        expected_result_by_index,
    )
    .await;
}

#[tokio::test]
async fn get_events_pending_not_up_to_date() {
    // As a special edge case, the function get_events doesn't return events if there are no
    // accepted blocks, even if there is a pending block. Therefore, we need to have a block in the
    // storage.
    let blocks_metadata = vec![BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]])];
    let pending_block_metadata = Some(BlockMetadata(vec![vec![DEFAULT_EVENT_METADATA]]));
    let is_pending_up_to_date = false;
    let expected_result_by_index = vec![(
        vec![EventIndex(
            TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)),
            EventIndexInTransactionOutput(0),
        )],
        None,
    )];
    test_get_events(
        blocks_metadata,
        pending_block_metadata,
        is_pending_up_to_date,
        EventFilter { chunk_size: 2, ..Default::default() },
        expected_result_by_index,
    )
    .await;
}

#[tokio::test]
async fn get_events_page_size_too_big() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    // Create the filter.
    let filter = EventFilter {
        from_block: None,
        to_block: None,
        continuation_token: None,
        chunk_size: get_test_rpc_config().max_events_chunk_size + 1,
        address: None,
        keys: vec![],
    };

    call_api_then_assert_and_validate_schema_for_err::<_, EventsChunk>(
        &module,
        "starknet_V0_6_getEvents",
        vec![Box::new(filter)],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &PAGE_SIZE_TOO_BIG.into(),
    )
    .await;
}

#[tokio::test]
async fn get_events_too_many_keys() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();
    let keys = (0..get_test_rpc_config().max_events_keys + 1)
        .map(|i| HashSet::from([EventKey(Felt::from(i as u128))]))
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

    call_api_then_assert_and_validate_schema_for_err::<_, EventsChunk>(
        &module,
        "starknet_V0_6_getEvents",
        vec![Box::new(filter)],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &TOO_MANY_KEYS_IN_FILTER.into(),
    )
    .await;
}

#[tokio::test]
async fn get_events_invalid_ct() {
    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();
    let block = starknet_api::block::Block::default();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block.header.block_number, &block.header)
        .unwrap()
        .append_body(block.header.block_number, block.body)
        .unwrap()
        .append_state_diff(
            block.header.block_number,
            starknet_api::state::StateDiff::default(),
            IndexMap::new(),
        )
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

    call_api_then_assert_and_validate_schema_for_err::<_, EventsChunk>(
        &module,
        "starknet_V0_6_getEvents",
        vec![Box::new(filter)],
        &VERSION,
        SpecFile::StarknetApiOpenrpc,
        &INVALID_CONTINUATION_TOKEN.into(),
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
            block_hash: BlockHash(Felt::ONE),
            block_number: BlockNumber(1),
            ..BlockHeader::default()
        },
        body: get_test_body(5, Some(5), None, None),
    };
    let mut state_diff = StateDiff::get_test_instance(&mut rng);
    // In the test instance both declared_classes and deprecated_declared_classes have an entry
    // with class hash 0x0, which is illegal.
    state_diff.deprecated_declared_classes = IndexMap::from([(
        ClassHash(Felt::TWO),
        starknet_api::deprecated_contract_class::ContractClass::get_test_instance(&mut rng),
    )]);
    // For checking the schema also for deprecated contract classes.
    state_diff.deployed_contracts.insert(ContractAddress(patricia_key!(0x2)), ClassHash(Felt::TWO));
    // TODO(yair): handle replaced classes.
    state_diff.replaced_classes.clear();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(parent_block.header.block_number, &parent_block.header)
        .unwrap()
        .update_starknet_version(&parent_block.header.block_number, &StarknetVersion::default())
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

    let gateway_config = get_test_rpc_config();
    let (server_address, _handle) = run_server(
        &gateway_config,
        get_test_highest_block(),
        get_test_pending_data(),
        get_test_pending_classes(),
        storage_reader,
        NODE_VERSION,
    )
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
        &VERSION,
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
        VERSION.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "State update is not valid.");

    let (address, _) = state_diff.deployed_contracts.get_index(0).unwrap();
    let res = send_request(
        server_address,
        "starknet_getClassAt",
        format!(r#"{{"block_number": 1}}, "0x{}""#, hex::encode(address.0.to_felt().to_bytes_be()))
            .as_str(),
        VERSION.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Class is not valid.");

    // TODO(dvir): Remove this after regenesis.
    // This checks the deployed deprecated class.
    let (address, _) = state_diff.deployed_contracts.get_index(1).unwrap();
    let res = send_request(
        server_address,
        "starknet_getClassAt",
        format!(r#"{{"block_number": 1}}, "0x{}""#, hex::encode(address.0.to_felt().to_bytes_be()))
            .as_str(),
        VERSION.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Class is not valid.");
}

async fn validate_block(header: &BlockHeader, server_address: SocketAddr, schema: &JSONSchema) {
    let res = send_request(
        server_address,
        "starknet_getBlockWithTxs",
        r#"{"block_number": 1}"#,
        VERSION.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Block with transactions is not valid.");

    let res = send_request(
        server_address,
        "starknet_getBlockWithTxHashes",
        format!(r#"{{"block_hash": "0x{}"}}"#, hex::encode(header.block_hash.0.to_bytes_be()))
            .as_str(),
        VERSION.name,
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
        VERSION.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Transaction is not valid.");

    let res = send_request(
        server_address,
        "starknet_getTransactionByHash",
        format!(r#""0x{}""#, hex::encode(tx_hash.0.to_bytes_be())).as_str(),
        VERSION.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Transaction is not valid.");

    let res = send_request(
        server_address,
        "starknet_getTransactionReceipt",
        format!(r#""0x{}""#, hex::encode(tx_hash.0.to_bytes_be())).as_str(),
        VERSION.name,
    )
    .await;
    assert!(validate_schema(schema, &res["result"]), "Transaction receipt is not valid.");

    let res =
        send_request(server_address, "starknet_getEvents", r#"{"chunk_size": 2}"#, VERSION.name)
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
            (ClassHash(Felt::ZERO), class_without_state_mutability),
            (ClassHash(Felt::ONE), class_with_state_mutability),
        ]),
        ..Default::default()
    };

    let (module, mut storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();
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
            "starknet_V0_6_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)),
                ClassHash(Felt::ZERO),
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
            "starknet_V0_6_getClass",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Hash(header.block_hash)),
                ClassHash(Felt::ONE),
            ),
        )
        .await
        .unwrap();
    let res_as_value = serde_json::to_value(res).unwrap();
    let entry = res_as_value["abi"][0].as_object().unwrap();
    assert_eq!(entry.get("stateMutability").unwrap().as_str().unwrap(), "view");
}

#[async_trait]
trait AddTransactionTest
where
    // This bound is a work-around for associated type bounds. It bounds
    // `Self::ClientTransaction::Error` to implement `Debug`.
    // associated type bounds is described here:
    // https://github.com/rust-lang/rfcs/blob/master/text/2289-associated-type-bounds.md
    <<Self as AddTransactionTest>::ClientTransaction as TryFrom<Self::Transaction>>::Error: Debug,
{
    type Transaction: GetTestInstance + Serialize + Clone + Send + Sync + 'static + Debug;
    type ClientTransaction: TryFrom<Self::Transaction> + Send + Debug;
    type Response: From<Self::ClientResponse>
        + for<'de> Deserialize<'de>
        + Eq
        + Debug
        + Clone
        + Send
        + Sync;
    type ClientResponse: GetTestInstance + Clone + Send;

    const METHOD_NAME: &'static str;

    fn expect_add_transaction(
        client_mock: &mut MockStarknetWriter,
        client_tx: Self::ClientTransaction,
        client_result: WriterClientResult<Self::ClientResponse>,
    );

    async fn test_positive_flow() {
        let mut rng = get_rng();
        let tx = Self::Transaction::get_test_instance(&mut rng);
        let client_resp = Self::ClientResponse::get_test_instance(&mut rng);
        let expected_resp = Self::Response::from(client_resp.clone());

        let mut client_mock = MockStarknetWriter::new();
        Self::expect_add_transaction(
            &mut client_mock,
            Self::ClientTransaction::try_from(tx.clone()).unwrap(),
            Ok(client_resp),
        );

        let (module, _) = get_test_rpc_server_and_storage_writer_from_params::<JsonRpcServerImpl>(
            Some(client_mock),
            None,
            None,
            None,
            None,
        );
        call_api_then_assert_and_validate_schema_for_result(
            &module,
            Self::METHOD_NAME,
            vec![Box::new(tx)],
            &VERSION,
            SpecFile::WriteApi,
            &expected_resp,
        )
        .await;
    }

    async fn test_internal_error() {
        let mut rng = get_rng();
        let tx = Self::Transaction::get_test_instance(&mut rng);
        let client_error = WriterClientError::ClientError(ClientError::BadResponseStatus {
            code: StatusCode::from_u16(404).unwrap(),
            message: "This site can’t be reached".to_owned(),
        });
        let expected_error = internal_server_error(&client_error);

        let mut client_mock = MockStarknetWriter::new();
        Self::expect_add_transaction(
            &mut client_mock,
            Self::ClientTransaction::try_from(tx.clone()).unwrap(),
            Err(client_error),
        );

        let (module, _) = get_test_rpc_server_and_storage_writer_from_params::<JsonRpcServerImpl>(
            Some(client_mock),
            None,
            None,
            None,
            None,
        );
        let result = module.call::<_, Self::Response>(Self::METHOD_NAME, [tx]).await;
        let jsonrpsee::core::Error::Call(error) = result.unwrap_err() else {
            panic!("Got an error which is not a call error");
        };
        assert_eq!(error, expected_error);
    }

    async fn test_known_starknet_error(
        known_starknet_error_code: KnownStarknetErrorCode,
        expected_error: JsonRpcError<String>,
    ) {
        let mut rng = get_rng();
        let tx = Self::Transaction::get_test_instance(&mut rng);
        const MESSAGE: &str = "message";
        let client_error =
            WriterClientError::ClientError(ClientError::StarknetError(StarknetError {
                code: StarknetErrorCode::KnownErrorCode(known_starknet_error_code),
                message: MESSAGE.to_owned(),
            }));

        let mut client_mock = MockStarknetWriter::new();
        Self::expect_add_transaction(
            &mut client_mock,
            Self::ClientTransaction::try_from(tx.clone()).unwrap(),
            Err(client_error),
        );

        let (module, _) = get_test_rpc_server_and_storage_writer_from_params::<JsonRpcServerImpl>(
            Some(client_mock),
            None,
            None,
            None,
            None,
        );
        let result = module.call::<_, Self::Response>(Self::METHOD_NAME, [tx]).await;
        let jsonrpsee::core::Error::Call(error) = result.unwrap_err() else {
            panic!("Got an error which is not a call error");
        };
        assert_eq!(error, expected_error.into());
    }

    async fn test_unexpected_error(known_starknet_error_code: KnownStarknetErrorCode) {
        let mut rng = get_rng();
        let tx = Self::Transaction::get_test_instance(&mut rng);
        const MESSAGE: &str = "message";
        let client_error =
            WriterClientError::ClientError(ClientError::StarknetError(StarknetError {
                code: StarknetErrorCode::KnownErrorCode(known_starknet_error_code),
                message: MESSAGE.to_owned(),
            }));

        let mut client_mock = MockStarknetWriter::new();
        Self::expect_add_transaction(
            &mut client_mock,
            Self::ClientTransaction::try_from(tx.clone()).unwrap(),
            Err(client_error),
        );

        let (module, _) = get_test_rpc_server_and_storage_writer_from_params::<JsonRpcServerImpl>(
            Some(client_mock),
            None,
            None,
            None,
            None,
        );
        let result = module.call::<_, Self::Response>(Self::METHOD_NAME, [tx]).await;
        let jsonrpsee::core::Error::Call(error) = result.unwrap_err() else {
            panic!("Got an error which is not a call error");
        };
        assert_eq!(error, unexpected_error(MESSAGE.to_owned()).into());
    }
}

struct AddInvokeTest {}
impl AddTransactionTest for AddInvokeTest {
    type Transaction = TypedInvokeTransaction;
    type ClientTransaction = ClientInvokeTransaction;
    type Response = AddInvokeOkResult;
    type ClientResponse = InvokeResponse;

    const METHOD_NAME: &'static str = "starknet_V0_6_addInvokeTransaction";

    fn expect_add_transaction(
        client_mock: &mut MockStarknetWriter,
        client_tx: Self::ClientTransaction,
        client_result: WriterClientResult<Self::ClientResponse>,
    ) {
        client_mock
            .expect_add_invoke_transaction()
            .times(1)
            .with(eq(client_tx))
            .return_once(move |_| client_result);
    }
}

struct AddDeployAccountTest {}
impl AddTransactionTest for AddDeployAccountTest {
    type Transaction = TypedDeployAccountTransaction;
    type ClientTransaction = ClientDeployAccountTransaction;
    type Response = AddDeployAccountOkResult;
    type ClientResponse = DeployAccountResponse;

    const METHOD_NAME: &'static str = "starknet_V0_6_addDeployAccountTransaction";

    fn expect_add_transaction(
        client_mock: &mut MockStarknetWriter,
        client_tx: Self::ClientTransaction,
        client_result: WriterClientResult<Self::ClientResponse>,
    ) {
        client_mock
            .expect_add_deploy_account_transaction()
            .times(1)
            .with(eq(client_tx))
            .return_once(move |_| client_result);
    }
}

struct AddDeclareTest {}
impl AddTransactionTest for AddDeclareTest {
    type Transaction = BroadcastedDeclareTransaction;
    type ClientTransaction = ClientDeclareTransaction;
    type Response = AddDeclareOkResult;
    type ClientResponse = DeclareResponse;

    const METHOD_NAME: &'static str = "starknet_V0_6_addDeclareTransaction";

    fn expect_add_transaction(
        client_mock: &mut MockStarknetWriter,
        client_tx: Self::ClientTransaction,
        client_result: WriterClientResult<Self::ClientResponse>,
    ) {
        client_mock
            .expect_add_declare_transaction()
            .times(1)
            .with(eq(client_tx))
            .return_once(move |_| client_result);
    }
}

// TODO(shahak): Test starknet error.

#[tokio::test]
async fn add_invoke_positive_flow() {
    AddInvokeTest::test_positive_flow().await;
}

#[tokio::test]
async fn add_invoke_internal_error() {
    AddInvokeTest::test_internal_error().await;
}

#[tokio::test]
async fn add_invoke_known_starknet_error() {
    AddInvokeTest::test_known_starknet_error(
        KnownStarknetErrorCode::DuplicatedTransaction,
        DUPLICATE_TX,
    )
    .await;
}

#[tokio::test]
async fn add_invoke_unexpected_error() {
    // Choosing error codes that map under the other transaction types into an expected error in
    // order to check that we call invoke's error conversion.
    AddInvokeTest::test_unexpected_error(KnownStarknetErrorCode::CompilationFailed).await;
    AddInvokeTest::test_unexpected_error(KnownStarknetErrorCode::UndeclaredClass).await;
}

#[tokio::test]
async fn add_deploy_account_positive_flow() {
    AddDeployAccountTest::test_positive_flow().await;
}

#[tokio::test]
async fn add_deploy_account_internal_error() {
    AddDeployAccountTest::test_internal_error().await;
}

#[tokio::test]
async fn add_deploy_account_known_starknet_error() {
    // Choosing an error code that maps under the other transaction types into an unexpected error
    // in order to check that we call deploy_account's error conversion.
    AddDeployAccountTest::test_known_starknet_error(
        KnownStarknetErrorCode::UndeclaredClass,
        CLASS_HASH_NOT_FOUND,
    )
    .await;
}

#[tokio::test]
async fn add_deploy_account_unexpected_error() {
    // Choosing an error code that maps under the other transaction types into an expected error in
    // order to check that we call deploy_account's error conversion.
    AddDeployAccountTest::test_unexpected_error(KnownStarknetErrorCode::CompilationFailed).await;
}

#[tokio::test]
async fn add_declare_positive_flow() {
    AddDeclareTest::test_positive_flow().await;
}

#[tokio::test]
async fn add_declare_internal_error() {
    AddDeclareTest::test_internal_error().await;
}

#[tokio::test]
async fn add_declare_known_starknet_error() {
    // Choosing an error code that maps under the other transaction types into an unexpected error
    // in order to check that we call declare's error conversion.
    AddDeclareTest::test_known_starknet_error(
        KnownStarknetErrorCode::CompilationFailed,
        COMPILATION_FAILED,
    )
    .await;
}

#[tokio::test]
async fn add_declare_unexpected_error() {
    // Choosing an error code that maps under the other transaction types into an expected error in
    // order to check that we call declare's error conversion.
    AddDeclareTest::test_unexpected_error(KnownStarknetErrorCode::UndeclaredClass).await;
}

#[test]
fn spec_api_methods_coverage() {
    let (module, _) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();
    let implemented_methods: Methods = module.into();
    let implemented_method_names = implemented_methods
        .method_names()
        .map(method_name_to_spec_method_name)
        .sorted()
        .collect::<Vec<_>>();
    let non_implemented_apis = ["starknet_pendingTransactions".to_string()];
    let method_names_in_spec = get_method_names_from_spec(&VERSION)
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
    assert!(method_names_in_spec.eq(&implemented_method_names));
}

auto_impl_get_test_instance! {
    pub struct PendingBlockHeader {
        pub parent_hash: BlockHash,
        pub sequencer_address: ContractAddress,
        pub timestamp: BlockTimestamp,
        pub l1_gas_price: ResourcePrice,
        pub starknet_version: String,
    }
    pub struct ResourcePrice {
        pub price_in_wei: GasPrice,
        pub price_in_fri: GasPrice,
    }
    pub enum TypedInvokeTransaction {
        Invoke(InvokeTransaction) = 0,
    }
    pub enum TypedDeployAccountTransaction {
        DeployAccount(DeployAccountTransaction) = 0,
    }
}
