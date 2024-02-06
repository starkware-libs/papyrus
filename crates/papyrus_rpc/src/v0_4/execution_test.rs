use std::env;
use std::fs::read_to_string;
use std::path::Path;
use std::sync::Arc;

use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::{indexmap, IndexMap};
use jsonrpsee::core::Error;
use jsonrpsee::RpcModule;
use lazy_static::lazy_static;
use papyrus_common::pending_classes::{ApiContractClass, PendingClasses, PendingClassesTrait};
use papyrus_common::state::{DeclaredClassHashEntry, DeployedContract, StorageEntry};
use papyrus_execution::execution_utils::selector_from_name;
use papyrus_execution::objects::{CallType, FunctionCall, Retdata, RevertReason};
use papyrus_execution::testing_instances::get_storage_var_address;
use papyrus_execution::ExecutableTransactionInput;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::StorageWriter;
use pretty_assertions::assert_eq;
use starknet_api::block::{
    BlockBody,
    BlockHash,
    BlockHeader,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::deprecated_contract_class::{
    ContractClass as SN_API_DeprecatedContractClass,
    EntryPointType,
};
use starknet_api::state::StateDiff;
use starknet_api::transaction::{
    Calldata,
    EventContent,
    Fee,
    L1HandlerTransaction,
    MessageToL1,
    TransactionHash,
    TransactionOffsetInBlock,
    TransactionVersion,
};
use starknet_api::{calldata, class_hash, contract_address, patricia_key};
use starknet_client::reader::objects::pending_data::{PendingBlock, PendingStateUpdate};
use starknet_client::reader::objects::state::StateDiff as ClientStateDiff;
use starknet_client::reader::objects::transaction::{
    IntermediateInvokeTransaction as ClientInvokeTransaction,
    Transaction as ClientTransaction,
    TransactionReceipt as ClientTransactionReceipt,
};
use starknet_client::reader::PendingData;
use starknet_types_core::felt::Felt;
use test_utils::{
    auto_impl_get_test_instance,
    get_number_of_variants,
    get_rng,
    read_json_file,
    GetTestInstance,
};
use tokio::sync::RwLock;

use super::api::api_impl::JsonRpcServerV0_4Impl;
use super::api::{
    decompress_program,
    FeeEstimate,
    SimulatedTransaction,
    SimulationFlag,
    TransactionTraceWithHash,
};
use super::broadcasted_transaction::{
    BroadcastedDeclareTransaction,
    BroadcastedDeclareV1Transaction,
    BroadcastedTransaction,
};
use super::error::{BLOCK_NOT_FOUND, CONTRACT_ERROR, CONTRACT_NOT_FOUND};
use super::execution::{
    DeclareTransactionTrace,
    DeployAccountTransactionTrace,
    FunctionInvocation,
    FunctionInvocationResult,
    InvokeTransactionTrace,
    L1HandlerTransactionTrace,
    TransactionTrace,
};
use super::transaction::{
    DeployAccountTransaction,
    InvokeTransaction,
    InvokeTransactionV1,
    MessageFromL1,
    TransactionVersion1,
};
use crate::api::{BlockHashOrNumber, BlockId, CallRequest, Tag};
use crate::test_utils::{
    call_and_validate_schema_for_result,
    call_api_then_assert_and_validate_schema_for_result,
    get_starknet_spec_api_schema_for_components,
    get_starknet_spec_api_schema_for_method_results,
    get_test_pending_classes,
    get_test_pending_data,
    get_test_rpc_config,
    get_test_rpc_server_and_storage_writer,
    get_test_rpc_server_and_storage_writer_from_params,
    validate_schema,
    SpecFile,
};
use crate::version_config::VERSION_0_4;

lazy_static! {
    pub static ref GAS_PRICE: GasPrice = GasPrice(100 * u128::pow(10, 9)); // Given in units of wei.
    pub static ref MAX_FEE: Fee = Fee(1000000 * GAS_PRICE.0);
    pub static ref BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234);
    pub static ref SEQUENCER_ADDRESS: ContractAddress = contract_address!(0xa);
    pub static ref DEPRECATED_CONTRACT_ADDRESS: ContractAddress = contract_address!(0x1);
    pub static ref CONTRACT_ADDRESS: ContractAddress = contract_address!(0x2);
    pub static ref ACCOUNT_CLASS_HASH: ClassHash = class_hash!(0x333);
    pub static ref ACCOUNT_ADDRESS: ContractAddress = contract_address!(0x444);
    pub static ref TEST_ERC20_CONTRACT_CLASS_HASH: ClassHash = class_hash!(0x1010);
    pub static ref TEST_ERC20_CONTRACT_ADDRESS: ContractAddress = contract_address!(0x1001);
    pub static ref ACCOUNT_INITIAL_BALANCE: Felt = Felt::from(2 * MAX_FEE.0);
    // TODO(yair): verify this is the correct fee, got this value by printing the result of the
    // call.
    pub static ref EXPECTED_FEE_ESTIMATE: FeeEstimate = FeeEstimate {
        gas_consumed: Felt::from_hex_unchecked("0x68b"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(167500000000000,),
    };

    // A message from L1 contract at address 0x987 to the contract at CONTRACT_ADDRESS that calls
    // the entry point "l1_handle" with the value 0x123, the retdata should be 0x123.
    pub static ref MESSAGE_FROM_L1: MessageFromL1 = MessageFromL1 {
        from_address: EthAddress::try_from(Felt::from_hex_unchecked(
            "0x987"
        ))
        .unwrap(),
        to_address: *CONTRACT_ADDRESS,
        entry_point_selector: selector_from_name("l1_handle"),
        payload: calldata![
            Felt::from_hex_unchecked("0x123")
        ],
    };
}

#[tokio::test]
async fn execution_call() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let key = Felt::from_hex_unchecked("0x1234");
    let value = Felt::from_hex_unchecked("0x18");

    call_api_then_assert_and_validate_schema_for_result(
        &module,
        "starknet_V0_4_call",
        vec![
            Box::new(CallRequest {
                contract_address: *DEPRECATED_CONTRACT_ADDRESS,
                entry_point_selector: selector_from_name("test_storage_read_write"),
                calldata: calldata![key, value],
            }),
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
        ],
        &VERSION_0_4,
        SpecFile::StarknetApiOpenrpc,
        &vec![value],
    )
    .await;

    // Calling a non-existent contract.
    let err = module
        .call::<_, Vec<Felt>>(
            "starknet_V0_4_call",
            (
                CallRequest {
                    contract_address: ContractAddress(patricia_key!(0x1234)),
                    entry_point_selector: selector_from_name("aaa"),
                    calldata: calldata![key, value],
                },
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == CONTRACT_NOT_FOUND.into());

    // Calling a non-existent block.
    let err = module
        .call::<_, Vec<Felt>>(
            "starknet_V0_4_call",
            (
                CallRequest {
                    contract_address: ContractAddress(patricia_key!(0x1234)),
                    entry_point_selector: selector_from_name("aaa"),
                    calldata: calldata![key, value],
                },
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(7))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());

    // Calling a non-existent function (contract error).
    let err = module
        .call::<_, Vec<Felt>>(
            "starknet_V0_4_call",
            (
                CallRequest {
                    contract_address: *DEPRECATED_CONTRACT_ADDRESS,
                    entry_point_selector: selector_from_name("aaa"),
                    calldata: calldata![key, value],
                },
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == CONTRACT_ERROR.into());

    // Test that the block context is passed correctly to blockifier.
    let mut calldata = get_calldata_for_test_execution_info(
        BlockNumber(0),
        *BLOCK_TIMESTAMP,
        *SEQUENCER_ADDRESS,
        &InvokeTransactionV1::default(),
        TransactionHash(Felt::ZERO),
        Some(Felt::ZERO),
    );
    // Calling the contract directly and not through the account contract.
    let contract_address = contract_address!(Arc::get_mut(&mut calldata.0).unwrap().remove(0));
    let entry_point_selector = EntryPointSelector(Arc::get_mut(&mut calldata.0).unwrap().remove(0));
    let _calldata_length = Arc::get_mut(&mut calldata.0).unwrap().remove(0);

    module
        .call::<_, Vec<Felt>>(
            "starknet_V0_4_call",
            (
                CallRequest { contract_address, entry_point_selector, calldata },
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn pending_execution_call() {
    let pending_data = get_test_pending_data();
    let pending_classes = get_test_pending_classes();
    write_block_0_as_pending(pending_data.clone(), pending_classes.clone()).await;
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerV0_4Impl,
    >(
        None, None, Some(pending_data), Some(pending_classes), None
    );
    write_empty_block(storage_writer);

    let key = Felt::from_hex_unchecked("0x1234");
    let value = Felt::from_hex_unchecked("0x18");

    let res = module
        .call::<_, Vec<Felt>>(
            "starknet_V0_4_call",
            (
                CallRequest {
                    contract_address: *DEPRECATED_CONTRACT_ADDRESS,
                    entry_point_selector: selector_from_name("test_storage_read_write"),
                    calldata: calldata![key, value],
                },
                BlockId::Tag(Tag::Pending),
            ),
        )
        .await
        .unwrap();

    assert_eq!(res, vec![value]);

    // Test that the block context is passed correctly to blockifier with a block number that is
    // after the latest block in the storage.
    let mut calldata = get_calldata_for_test_execution_info(
        BlockNumber(1),
        *BLOCK_TIMESTAMP,
        *SEQUENCER_ADDRESS,
        &InvokeTransactionV1::default(),
        TransactionHash(Felt::ZERO),
        Some(Felt::ZERO),
    );
    // Calling the contract directly and not through the account contract.
    let contract_address = contract_address!(Arc::get_mut(&mut calldata.0).unwrap().remove(0));
    let entry_point_selector = EntryPointSelector(Arc::get_mut(&mut calldata.0).unwrap().remove(0));
    let _calldata_length = Arc::get_mut(&mut calldata.0).unwrap().remove(0);

    module
        .call::<_, Vec<Felt>>(
            "starknet_V0_4_call",
            (
                CallRequest { contract_address, entry_point_selector, calldata },
                BlockId::Tag(Tag::Pending),
            ),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn call_estimate_fee() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let account_address = ContractAddress(patricia_key!(0x444));

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion1::default(),
        sender_address: account_address,
        calldata: calldata![
            DEPRECATED_CONTRACT_ADDRESS.0.to_felt(), // Contract address.
            selector_from_name("return_result").0,   // EP selector.
            Felt::ONE,                               // Calldata length.
            Felt::TWO                                // Calldata: num.
        ],
        ..Default::default()
    }));

    call_api_then_assert_and_validate_schema_for_result(
        &module,
        "starknet_V0_4_estimateFee",
        vec![
            Box::new(vec![invoke.clone()]),
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
        ],
        &VERSION_0_4,
        SpecFile::StarknetApiOpenrpc,
        &vec![EXPECTED_FEE_ESTIMATE.clone()],
    )
    .await;

    // Test that calling the same transaction with a different block context with a different gas
    // price produces a different fee.
    let res = module
        .call::<_, Vec<FeeEstimate>>(
            "starknet_V0_4_estimateFee",
            (vec![invoke], BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))),
        )
        .await
        .unwrap();
    assert_ne!(res, vec![EXPECTED_FEE_ESTIMATE.clone()]);

    // TODO(shahak): Write a new contract and test execution info. The reason we can't do this with
    // the current contract is that the transaction hash appears in the calldata and thus it is
    // calculated inside the hash.
}

#[tokio::test]
async fn pending_call_estimate_fee() {
    let pending_data = get_test_pending_data();
    let pending_classes = get_test_pending_classes();
    write_block_0_as_pending(pending_data.clone(), pending_classes.clone()).await;
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerV0_4Impl,
    >(
        None, None, Some(pending_data), Some(pending_classes), None
    );
    write_empty_block(storage_writer);

    let account_address = ContractAddress(patricia_key!(0x444));

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion1::default(),
        sender_address: account_address,
        calldata: calldata![
            DEPRECATED_CONTRACT_ADDRESS.0.to_felt(), // Contract address.
            selector_from_name("return_result").0,   // EP selector.
            Felt::ONE,                               // Calldata length.
            Felt::TWO                                // Calldata: num.
        ],
        ..Default::default()
    }));

    let res = module
        .call::<_, Vec<FeeEstimate>>(
            "starknet_V0_4_estimateFee",
            (vec![invoke.clone()], BlockId::Tag(Tag::Pending)),
        )
        .await
        .unwrap();
    assert_eq!(res, vec![EXPECTED_FEE_ESTIMATE.clone()]);

    // TODO(shahak): Write a new contract and test execution info. The reason we can't do this with
    // the current contract is that the transaction hash appears in the calldata and thus it is
    // calculated inside the hash.
}

#[tokio::test]
async fn call_simulate() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    test_call_simulate(
        &module,
        BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
        BlockNumber(0),
    )
    .await;
}

#[tokio::test]
async fn pending_call_simulate() {
    let pending_data = get_test_pending_data();
    let pending_classes = get_test_pending_classes();
    write_block_0_as_pending(pending_data.clone(), pending_classes.clone()).await;
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerV0_4Impl,
    >(
        None, None, Some(pending_data), Some(pending_classes), None
    );
    write_empty_block(storage_writer);

    test_call_simulate(&module, BlockId::Tag(Tag::Pending), BlockNumber(1)).await;
}

// Test call_simulate. Assumes that the given block is equal to block number 0 that is returned
// from the function `prepare_storage_for_execution`.
async fn test_call_simulate(
    module: &RpcModule<JsonRpcServerV0_4Impl>,
    block_id: BlockId,
    block_context_number: BlockNumber,
) {
    let mut invoke_v1 = InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion1::default(),
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![
            DEPRECATED_CONTRACT_ADDRESS.0.to_felt(), // Contract address.
            selector_from_name("return_result").0,   // EP selector.
            Felt::ONE,                               // Calldata length.
            Felt::TWO                                // Calldata: num.
        ],
        ..Default::default()
    };
    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(invoke_v1.clone()));

    let mut res = call_and_validate_schema_for_result::<_, Vec<SimulatedTransaction>>(
        module,
        "starknet_V0_4_simulateTransactions",
        vec![Box::new(block_id), Box::new(vec![invoke]), Box::<Vec<SimulationFlag>>::default()],
        &VERSION_0_4,
        SpecFile::TraceApi,
    )
    .await;

    assert_eq!(res.len(), 1);

    let simulated_tx = res.pop().unwrap();

    assert_eq!(simulated_tx.fee_estimation, *EXPECTED_FEE_ESTIMATE);

    assert_matches!(simulated_tx.transaction_trace, TransactionTrace::Invoke(_));

    let TransactionTrace::Invoke(invoke_trace) = simulated_tx.transaction_trace else {
        unreachable!();
    };

    assert_matches!(invoke_trace.validate_invocation, Some(_));
    assert_matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_));
    assert_matches!(invoke_trace.fee_transfer_invocation, Some(_));

    // Test that the block context is passed correctly to blockifier.
    let calldata = get_calldata_for_test_execution_info(
        block_context_number,
        *BLOCK_TIMESTAMP,
        *SEQUENCER_ADDRESS,
        &invoke_v1,
        // Because the transaction hash depends on the calldata and the calldata needs to contain
        // the transaction hash, there's no way to put the correct hash here. Instead, we'll check
        // that the function `test_get_execution_info` fails on the transaction hash validation.
        TransactionHash(Felt::ZERO),
        None,
    );
    invoke_v1.calldata = calldata;

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(invoke_v1));

    let res = module
        .call::<_, Vec<SimulatedTransaction>>(
            "starknet_V0_4_simulateTransactions",
            (block_id, vec![invoke], Vec::<SimulationFlag>::new()),
        )
        .await
        .unwrap();

    let TransactionTrace::Invoke(invoke_trace) = &res[0].transaction_trace else {
        panic!("Got a non-invoke transaction trace from an invoke transaction.");
    };
    // As described above, we want to check that `test_get_execution_info` fails on the transaction
    // hash validation (which is done after the block context validation).
    let FunctionInvocationResult::Err(RevertReason::RevertReason(error_str)) =
        &invoke_trace.execute_invocation
    else {
        panic!("Expected call to test_get_execution_info to fail.");
    };
    assert!(error_str.contains("TX_INFO_MISMATCH"));
}

#[tokio::test]
async fn call_simulate_skip_validate() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion1::default(),
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![
            DEPRECATED_CONTRACT_ADDRESS.0.to_felt(), // Contract address.
            selector_from_name("return_result").0,   // EP selector.
            Felt::ONE,                               // Calldata length.
            Felt::TWO                                // Calldata: num.
        ],
        ..Default::default()
    }));

    let mut res = call_and_validate_schema_for_result::<_, Vec<SimulatedTransaction>>(
        &module,
        "starknet_V0_4_simulateTransactions",
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
            Box::new(vec![invoke]),
            Box::new(vec![SimulationFlag::SkipValidate]),
        ],
        &VERSION_0_4,
        SpecFile::TraceApi,
    )
    .await;

    assert_eq!(res.len(), 1);

    let simulated_tx = res.pop().unwrap();

    assert_eq!(simulated_tx.fee_estimation, *EXPECTED_FEE_ESTIMATE);

    assert_matches!(simulated_tx.transaction_trace, TransactionTrace::Invoke(_));

    let TransactionTrace::Invoke(invoke_trace) = simulated_tx.transaction_trace else {
        unreachable!();
    };

    assert_matches!(invoke_trace.validate_invocation, None);
    assert_matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_));
    assert_matches!(invoke_trace.fee_transfer_invocation, Some(_));
}

#[tokio::test]
async fn call_simulate_skip_fee_charge() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion1::default(),
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![
            DEPRECATED_CONTRACT_ADDRESS.0.to_felt(), // Contract address.
            selector_from_name("return_result").0,   // EP selector.
            Felt::ONE,                               // Calldata length.
            Felt::TWO                                // Calldata: num.
        ],
        ..Default::default()
    }));

    let mut res = call_and_validate_schema_for_result::<_, Vec<SimulatedTransaction>>(
        &module,
        "starknet_V0_4_simulateTransactions",
        vec![
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
            Box::new(vec![invoke]),
            Box::new(vec![SimulationFlag::SkipFeeCharge]),
        ],
        &VERSION_0_4,
        SpecFile::TraceApi,
    )
    .await;

    assert_eq!(res.len(), 1);

    let simulated_tx = res.pop().unwrap();

    assert_eq!(simulated_tx.fee_estimation, *EXPECTED_FEE_ESTIMATE);

    assert_matches!(simulated_tx.transaction_trace, TransactionTrace::Invoke(_));

    let TransactionTrace::Invoke(invoke_trace) = simulated_tx.transaction_trace else {
        unreachable!();
    };

    assert_matches!(invoke_trace.validate_invocation, Some(_));
    assert_matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_));
    assert_matches!(invoke_trace.fee_transfer_invocation, None);
}

// TODO(shahak): Add test for trace_transaction that doesn't depend on trace_block_transactions
#[tokio::test]
async fn trace_block_transactions_regular_and_pending() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    let mut writer = prepare_storage_for_execution(storage_writer);

    let tx_hash1 = TransactionHash(Felt::from_hex_unchecked("0x1234"));
    let tx_hash2 = TransactionHash(Felt::from_hex_unchecked("0x5678"));

    let client_tx1 = ClientTransaction::Invoke(ClientInvokeTransaction {
        max_fee: Some(*MAX_FEE),
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![
            DEPRECATED_CONTRACT_ADDRESS.0.to_felt(), // Contract address.
            selector_from_name("return_result").0,   // EP selector.
            Felt::ONE,                               // Calldata length.
            Felt::TWO                                // Calldata: num.
        ],
        nonce: Some(Nonce(Felt::ZERO)),
        version: TransactionVersion::ONE,
        ..Default::default()
    });
    let tx1: starknet_api::transaction::Transaction = client_tx1.clone().try_into().unwrap();
    let client_tx2 = ClientTransaction::Invoke(ClientInvokeTransaction {
        max_fee: Some(*MAX_FEE),
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![
            DEPRECATED_CONTRACT_ADDRESS.0.to_felt(), // Contract address.
            selector_from_name("return_result").0,   // EP selector.
            Felt::ONE,                               // Calldata length.
            Felt::TWO                                // Calldata: num.
        ],
        nonce: Some(Nonce(Felt::ONE)),
        version: TransactionVersion::ONE,
        ..Default::default()
    });
    let tx2: starknet_api::transaction::Transaction = client_tx2.clone().try_into().unwrap();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(2),
            &BlockHeader {
                eth_l1_gas_price: *GAS_PRICE,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                block_hash: BlockHash(Felt::TWO),
                parent_hash: BlockHash(Felt::ONE),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(
            BlockNumber(2),
            BlockBody {
                transactions: vec![tx1, tx2],
                transaction_outputs: vec![starknet_api::transaction::TransactionOutput::Invoke(
                    starknet_api::transaction::InvokeTransactionOutput::default(),
                )],
                transaction_hashes: vec![tx_hash1, tx_hash2],
            },
        )
        .unwrap()
        .append_state_diff(
            BlockNumber(2),
            StateDiff {
                nonces: indexmap!(*ACCOUNT_ADDRESS => Nonce(Felt::TWO)),
                ..Default::default()
            },
            IndexMap::new(),
        )
        .unwrap()
        .commit()
        .unwrap();

    let tx_1_trace = call_and_validate_schema_for_result::<_, TransactionTrace>(
        &module,
        "starknet_V0_4_traceTransaction",
        vec![Box::new(tx_hash1)],
        &VERSION_0_4,
        SpecFile::TraceApi,
    )
    .await;

    assert_matches!(tx_1_trace, TransactionTrace::Invoke(_));

    let tx_2_trace = module
        .call::<_, TransactionTrace>("starknet_V0_4_traceTransaction", [tx_hash2])
        .await
        .unwrap();

    assert_matches!(tx_2_trace, TransactionTrace::Invoke(_));

    let res = call_and_validate_schema_for_result::<_, Vec<TransactionTraceWithHash>>(
        &module,
        "starknet_V0_4_traceBlockTransactions",
        vec![Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2))))],
        &VERSION_0_4,
        SpecFile::TraceApi,
    )
    .await;

    assert_eq!(res.len(), 2);
    assert_eq!(res[0].trace_root, tx_1_trace);
    assert_eq!(res[0].transaction_hash, tx_hash1);
    assert_eq!(res[1].trace_root, tx_2_trace);
    assert_eq!(res[1].transaction_hash, tx_hash2);

    // Ask for trace of pending block.
    // Create a new storage without the last block and put the last block as pending

    let pending_data = get_test_pending_data();
    *pending_data.write().await = PendingData {
        block: PendingBlock {
            eth_l1_gas_price: *GAS_PRICE,
            sequencer_address: *SEQUENCER_ADDRESS,
            timestamp: *BLOCK_TIMESTAMP,
            parent_block_hash: BlockHash(Felt::ONE),
            transactions: vec![client_tx1, client_tx2],
            transaction_receipts: vec![
                ClientTransactionReceipt {
                    transaction_index: TransactionOffsetInBlock(0),
                    transaction_hash: tx_hash1,
                    ..Default::default()
                },
                ClientTransactionReceipt {
                    transaction_index: TransactionOffsetInBlock(0),
                    transaction_hash: tx_hash2,
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
        state_update: PendingStateUpdate {
            old_root: Default::default(),
            state_diff: ClientStateDiff {
                nonces: indexmap!(*ACCOUNT_ADDRESS => Nonce(Felt::TWO)),
                ..Default::default()
            },
        },
    };

    let (module, storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerV0_4Impl,
    >(None, None, Some(pending_data), None, None);

    prepare_storage_for_execution(storage_writer);

    let res = module
        .call::<_, Vec<TransactionTraceWithHash>>(
            "starknet_V0_4_traceBlockTransactions",
            [BlockId::Tag(Tag::Pending)],
        )
        .await
        .unwrap();

    assert_eq!(res.len(), 2);
    assert_eq!(res[0].trace_root, tx_1_trace);
    assert_eq!(res[0].transaction_hash, tx_hash1);
    assert_eq!(res[1].trace_root, tx_2_trace);
    assert_eq!(res[1].transaction_hash, tx_hash2);

    // Ask for trace of transactions in the pending block.
    let pending_tx_1_trace = module
        .call::<_, TransactionTrace>("starknet_V0_4_traceTransaction", [tx_hash1])
        .await
        .unwrap();
    assert_eq!(pending_tx_1_trace, tx_1_trace);
    let pending_tx_2_trace = module
        .call::<_, TransactionTrace>("starknet_V0_4_traceTransaction", [tx_hash2])
        .await
        .unwrap();
    assert_eq!(pending_tx_2_trace, tx_2_trace);
}

#[tokio::test]
async fn trace_block_transactions_and_trace_transaction_execution_context() {
    let tx_hash1 = TransactionHash(Felt::from_hex_unchecked("0x1234"));
    let tx_hash2 = TransactionHash(Felt::from_hex_unchecked("0x5678"));

    let mut invoke_tx1 = starknet_api::transaction::InvokeTransactionV1 {
        max_fee: *MAX_FEE,
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![],
        nonce: Nonce(Felt::ZERO),
        ..Default::default()
    };
    let mut invoke_tx2 = starknet_api::transaction::InvokeTransactionV1 {
        max_fee: *MAX_FEE,
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![],
        nonce: Nonce(Felt::ONE),
        ..Default::default()
    };

    let fix_calldata_of_invoke_tx =
        |invoke_tx: &mut starknet_api::transaction::InvokeTransactionV1, tx_hash| {
            let tx: super::transaction::Transaction =
                starknet_api::transaction::Transaction::Invoke(
                    starknet_api::transaction::InvokeTransaction::V1(invoke_tx.clone()),
                )
                .try_into()
                .unwrap();
            let super::transaction::Transaction::Invoke(InvokeTransaction::Version1(rpc_invoke_v1)) =
                tx
            else {
                panic!(
                    "Converting an InvokeV1 client transaction to a starknet api transaction did \
                     not yield an InvokeV1 transaction"
                );
            };
            invoke_tx.calldata = get_calldata_for_test_execution_info(
                BlockNumber(2),
                *BLOCK_TIMESTAMP,
                *SEQUENCER_ADDRESS,
                &rpc_invoke_v1,
                tx_hash,
                None,
            );
        };
    fix_calldata_of_invoke_tx(&mut invoke_tx1, tx_hash1);
    fix_calldata_of_invoke_tx(&mut invoke_tx2, tx_hash2);
    let tx1 = starknet_api::transaction::Transaction::Invoke(
        starknet_api::transaction::InvokeTransaction::V1(invoke_tx1),
    );
    let tx2 = starknet_api::transaction::Transaction::Invoke(
        starknet_api::transaction::InvokeTransaction::V1(invoke_tx2),
    );

    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    let mut writer = prepare_storage_for_execution(storage_writer);

    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(2),
            &BlockHeader {
                block_number: BlockNumber(2),
                eth_l1_gas_price: *GAS_PRICE,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                block_hash: BlockHash(Felt::TWO),
                parent_hash: BlockHash(Felt::ONE),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(
            BlockNumber(2),
            BlockBody {
                transactions: vec![tx1, tx2],
                transaction_outputs: vec![starknet_api::transaction::TransactionOutput::Invoke(
                    starknet_api::transaction::InvokeTransactionOutput::default(),
                )],
                transaction_hashes: vec![tx_hash1, tx_hash2],
            },
        )
        .unwrap()
        .append_state_diff(
            BlockNumber(2),
            StateDiff {
                nonces: indexmap!(*ACCOUNT_ADDRESS => Nonce(Felt::TWO)),
                ..Default::default()
            },
            IndexMap::new(),
        )
        .unwrap()
        .commit()
        .unwrap();

    let validate_result = |res| {
        assert_matches!(
            &res,
            TransactionTrace::Invoke(invoke_trace)
            if matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_))
        );
    };

    validate_result(
        module
            .call::<_, TransactionTrace>("starknet_V0_4_traceTransaction", [tx_hash1])
            .await
            .unwrap(),
    );

    validate_result(
        module
            .call::<_, TransactionTrace>("starknet_V0_4_traceTransaction", [tx_hash2])
            .await
            .unwrap(),
    );

    let res = module
        .call::<_, Vec<TransactionTraceWithHash>>(
            "starknet_V0_4_traceBlockTransactions",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2)))],
        )
        .await
        .unwrap();
    validate_result(res[0].trace_root.clone());
    validate_result(res[1].trace_root.clone());
}

#[tokio::test]
async fn pending_trace_block_transactions_and_trace_transaction_execution_context() {
    let tx_hash1 = TransactionHash(Felt::from_hex_unchecked("0x1234"));
    let tx_hash2 = TransactionHash(Felt::from_hex_unchecked("0x5678"));

    let mut client_invoke_tx1 = ClientInvokeTransaction {
        max_fee: Some(*MAX_FEE),
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![],
        nonce: Some(Nonce(Felt::ZERO)),
        version: TransactionVersion::ONE,
        ..Default::default()
    };
    let mut client_invoke_tx2 = ClientInvokeTransaction {
        max_fee: Some(*MAX_FEE),
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![],
        nonce: Some(Nonce(Felt::ONE)),
        version: TransactionVersion::ONE,
        ..Default::default()
    };

    let fix_calldata_of_client_invoke_tx = |client_invoke_tx: &mut ClientInvokeTransaction,
                                            tx_hash| {
        let starknet_api_tx: starknet_api::transaction::Transaction =
            ClientTransaction::Invoke(client_invoke_tx.clone()).try_into().unwrap();
        let tx: super::transaction::Transaction = starknet_api_tx.try_into().unwrap();
        let super::transaction::Transaction::Invoke(InvokeTransaction::Version1(invoke_v1)) = tx
        else {
            panic!(
                "Converting an InvokeV1 client transaction to a starknet api transaction did not \
                 yield an InvokeV1 transaction"
            );
        };
        client_invoke_tx.calldata = get_calldata_for_test_execution_info(
            BlockNumber(2),
            *BLOCK_TIMESTAMP,
            *SEQUENCER_ADDRESS,
            &invoke_v1,
            tx_hash,
            None,
        );
    };
    fix_calldata_of_client_invoke_tx(&mut client_invoke_tx1, tx_hash1);
    fix_calldata_of_client_invoke_tx(&mut client_invoke_tx2, tx_hash2);
    let client_tx1 = ClientTransaction::Invoke(client_invoke_tx1);
    let client_tx2 = ClientTransaction::Invoke(client_invoke_tx2);

    let pending_data = get_test_pending_data();
    *pending_data.write().await = PendingData {
        block: PendingBlock {
            eth_l1_gas_price: *GAS_PRICE,
            sequencer_address: *SEQUENCER_ADDRESS,
            timestamp: *BLOCK_TIMESTAMP,
            parent_block_hash: BlockHash(Felt::ONE),
            transactions: vec![client_tx1, client_tx2],
            transaction_receipts: vec![
                ClientTransactionReceipt {
                    transaction_index: TransactionOffsetInBlock(0),
                    transaction_hash: tx_hash1,
                    ..Default::default()
                },
                ClientTransactionReceipt {
                    transaction_index: TransactionOffsetInBlock(0),
                    transaction_hash: tx_hash2,
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
        state_update: PendingStateUpdate {
            old_root: Default::default(),
            state_diff: ClientStateDiff {
                nonces: indexmap!(*ACCOUNT_ADDRESS => Nonce(Felt::TWO)),
                ..Default::default()
            },
        },
    };

    let (module, storage_writer) = get_test_rpc_server_and_storage_writer_from_params::<
        JsonRpcServerV0_4Impl,
    >(None, None, Some(pending_data), None, None);

    prepare_storage_for_execution(storage_writer);

    let validate_result = |res| {
        assert_matches!(
            &res,
            TransactionTrace::Invoke(invoke_trace)
            if matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_))
        );
    };

    validate_result(
        module
            .call::<_, TransactionTrace>("starknet_V0_4_traceTransaction", [tx_hash1])
            .await
            .unwrap(),
    );

    validate_result(
        module
            .call::<_, TransactionTrace>("starknet_V0_4_traceTransaction", [tx_hash2])
            .await
            .unwrap(),
    );

    let res = module
        .call::<_, Vec<TransactionTraceWithHash>>(
            "starknet_V0_4_traceBlockTransactions",
            [BlockId::Tag(Tag::Pending)],
        )
        .await
        .unwrap();
    validate_result(res[0].trace_root.clone());
    validate_result(res[1].trace_root.clone());
}

#[test]
fn message_from_l1_to_l1_handler_tx() {
    let l1_handler_tx = L1HandlerTransaction::from(MESSAGE_FROM_L1.clone());
    assert_eq!(l1_handler_tx.version, TransactionVersion::ONE);
    assert_eq!(l1_handler_tx.contract_address, *CONTRACT_ADDRESS);
    assert_eq!(l1_handler_tx.entry_point_selector, selector_from_name("l1_handle"));
    // The first item of calldata is the from_address.
    let from_address = EthAddress::try_from(*l1_handler_tx.calldata.0.first().unwrap()).unwrap();
    assert_eq!(from_address, MESSAGE_FROM_L1.from_address);
    let rest_of_calldata = &l1_handler_tx.calldata.0[1..];
    assert_eq!(rest_of_calldata, MESSAGE_FROM_L1.payload.0.as_slice());
}

#[tokio::test]
async fn call_estimate_message_fee() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();
    prepare_storage_for_execution(storage_writer);

    // TODO(yair): get a l1_handler entry point that actually does something and check that the fee
    // is correct.
    let expected_fee_estimate =
        FeeEstimate { gas_consumed: Felt::ZERO, gas_price: *GAS_PRICE, overall_fee: Fee(0) };

    call_api_then_assert_and_validate_schema_for_result(
        &module,
        "starknet_V0_4_estimateMessageFee",
        vec![
            Box::new(MESSAGE_FROM_L1.clone()),
            Box::new(BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
        ],
        &VERSION_0_4,
        SpecFile::StarknetApiOpenrpc,
        &expected_fee_estimate,
    )
    .await;
}

#[test]
fn broadcasted_to_executable_declare_v1() {
    let mut rng = get_rng();
    let mut tx = BroadcastedDeclareV1Transaction::get_test_instance(&mut rng);
    tx.contract_class.compressed_program = get_test_compressed_program();
    let broadcasted_declare_v1 =
        BroadcastedTransaction::Declare(BroadcastedDeclareTransaction::V1(tx));
    assert_matches!(
        broadcasted_declare_v1.try_into(),
        Ok(ExecutableTransactionInput::DeclareV1(_tx, _class, _only_query))
    );
}

#[test]
fn validate_fee_estimation_schema() {
    let mut rng = get_rng();
    let fee_estimate = FeeEstimate::get_test_instance(&mut rng);
    let schema = get_starknet_spec_api_schema_for_components(
        &[(SpecFile::StarknetApiOpenrpc, &["FEE_ESTIMATE"])],
        &VERSION_0_4,
    );
    let serialized = serde_json::to_value(fee_estimate).unwrap();
    assert!(validate_schema(&schema, &serialized));
}

#[test]
fn validate_transaction_trace_with_hash_schema() {
    let mut rng = get_rng();
    let txs_with_trace = Vec::<TransactionTraceWithHash>::get_test_instance(&mut rng);
    let serialized = serde_json::to_value(txs_with_trace).unwrap();
    let schema = get_starknet_spec_api_schema_for_method_results(
        &[(SpecFile::TraceApi, &["starknet_traceBlockTransactions"])],
        &VERSION_0_4,
    );
    assert!(validate_schema(&schema, &serialized));
}

#[test]
fn validate_transaction_trace_schema() {
    let mut rng = get_rng();
    let schema = get_starknet_spec_api_schema_for_components(
        &[(SpecFile::TraceApi, &["TRANSACTION_TRACE"])],
        &VERSION_0_4,
    );

    let invoke_trace =
        TransactionTrace::Invoke(InvokeTransactionTrace::get_test_instance(&mut rng));
    let serialized = serde_json::to_value(invoke_trace).unwrap();
    assert!(validate_schema(&schema, &serialized));

    let declare_trace =
        TransactionTrace::Declare(DeclareTransactionTrace::get_test_instance(&mut rng));
    let serialized = serde_json::to_value(declare_trace).unwrap();
    assert!(validate_schema(&schema, &serialized));

    let deploy_account_trace =
        TransactionTrace::DeployAccount(DeployAccountTransactionTrace::get_test_instance(&mut rng));
    let serialized = serde_json::to_value(deploy_account_trace).unwrap();
    assert!(validate_schema(&schema, &serialized));

    let l1_handler_trace =
        TransactionTrace::L1Handler(L1HandlerTransactionTrace::get_test_instance(&mut rng));
    let serialized = serde_json::to_value(l1_handler_trace).unwrap();
    assert!(validate_schema(&schema, &serialized));
}

#[test]
fn broadcasted_to_executable_deploy_account() {
    let mut rng = get_rng();
    let broadcasted_deploy_account = BroadcastedTransaction::DeployAccount(
        DeployAccountTransaction::get_test_instance(&mut rng),
    );
    assert_matches!(
        broadcasted_deploy_account.try_into(),
        Ok(ExecutableTransactionInput::DeployAccount(_tx, _only_query))
    );
}

#[test]
fn broadcasted_to_executable_invoke() {
    let mut rng = get_rng();
    let broadcasted_deploy_account =
        BroadcastedTransaction::Invoke(InvokeTransaction::get_test_instance(&mut rng));
    assert_matches!(
        broadcasted_deploy_account.try_into(),
        Ok(ExecutableTransactionInput::Invoke(_tx, _only_query))
    );
}

#[test]
fn get_decompressed_program() {
    let compressed = get_test_compressed_program();
    let decompressed = decompress_program(&compressed);
    decompressed.expect("Couldn't decompress program");
}

fn get_test_compressed_program() -> String {
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join("base64_compressed_program.txt");
    read_to_string(path).expect("Couldn't read compressed program")
}

auto_impl_get_test_instance! {
    pub struct FeeEstimate {
        pub gas_consumed: Felt,
        pub gas_price: GasPrice,
        pub overall_fee: Fee,
    }

    pub struct TransactionTraceWithHash {
        pub transaction_hash: TransactionHash,
        pub trace_root: TransactionTrace,
    }
}

/// Get calldata for invoking the function `test_execution_info` in the contract located in
/// `casm.json`. The function `test_execution_info` receives the expected block context and
/// transaction context and validates first the block context and then the transaction context. The
/// returned calldata will also contain the contract address, entry point selector and calldata
/// length so that it can be used from an account contract. If you want to call the function
/// directly, remove the first 3 arguments of the calldata.
fn get_calldata_for_test_execution_info(
    expected_block_number: BlockNumber,
    expected_block_timestamp: BlockTimestamp,
    expected_sequencer_address: ContractAddress,
    invoke_tx: &InvokeTransactionV1,
    tx_hash: TransactionHash,
    override_tx_version: Option<Felt>,
) -> Calldata {
    let entry_point_selector = selector_from_name("test_get_execution_info");
    let expected_block_number = Felt::from(expected_block_number.0);
    let expected_block_timestamp = Felt::from(expected_block_timestamp.0);
    let expected_sequencer_address = expected_sequencer_address.0.to_felt();
    let expected_caller_address = invoke_tx.sender_address.0.to_felt();
    let expected_contract_address = CONTRACT_ADDRESS.0.to_felt();
    let expected_transaction_version = override_tx_version.unwrap_or(Felt::ONE);
    let expected_signature = invoke_tx.signature.0.clone();
    let expected_transaction_hash = tx_hash.0;
    let expected_chain_id = Felt::from_hex(&*(get_test_rpc_config().chain_id.as_hex())).unwrap();
    let expected_nonce = invoke_tx.nonce.0;
    let expected_max_fee = Felt::from(invoke_tx.max_fee.0);
    let expected_resource_bounds_length = Felt::ZERO;
    let expected_tip = Felt::ZERO;
    let expected_paymaster_data = Felt::ZERO;
    let expected_nonce_da = Felt::ZERO;
    let expected_fee_da = Felt::ZERO;
    let expected_account_data = Felt::ZERO;

    let calldata = [
        vec![
            expected_block_number,
            expected_block_timestamp,
            expected_sequencer_address,
            expected_transaction_version,
            expected_caller_address,
            expected_max_fee,
            Felt::from(expected_signature.len() as u64),
        ],
        expected_signature,
        vec![
            expected_transaction_hash,
            expected_chain_id,
            expected_nonce,
            expected_resource_bounds_length,
            expected_tip,
            expected_paymaster_data,
            expected_nonce_da,
            expected_fee_da,
            expected_account_data,
            expected_caller_address,
            expected_contract_address,
            Felt::from(entry_point_selector.0),
        ],
    ]
    .iter()
    .flatten()
    .cloned()
    .collect::<Vec<_>>();

    Calldata(Arc::new(
        [
            vec![
                CONTRACT_ADDRESS.0.to_felt(),
                entry_point_selector.0,
                Felt::from(calldata.len() as u64),
            ],
            calldata,
        ]
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>(),
    ))
}

// Write into the pending block the first block that the function `prepare_storage_for_execution`
// writes to the storage.
async fn write_block_0_as_pending(
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
) {
    let class1 = serde_json::from_value::<SN_API_DeprecatedContractClass>(read_json_file(
        "deprecated_class.json",
    ))
    .unwrap();
    let class_hash1 = class_hash!(0x1);

    let class2 = starknet_api::state::ContractClass::default();
    let casm = serde_json::from_value::<CasmContractClass>(read_json_file("casm.json")).unwrap();
    let class_hash2 = class_hash!(0x2);
    let compiled_class_hash = CompiledClassHash(Felt::default());

    let account_class = serde_json::from_value(read_json_file("account_class.json")).unwrap();
    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[ACCOUNT_ADDRESS.0.to_felt()]);

    let fee_contract_class = serde_json::from_value::<SN_API_DeprecatedContractClass>(
        read_json_file("erc20_fee_contract_class.json"),
    )
    .unwrap();
    let minter_var_address = get_storage_var_address("permitted_minter", &[]);

    let mut pending_classes_ref = pending_classes.write().await;
    pending_classes_ref.add_class(class_hash2, ApiContractClass::ContractClass(class2));
    pending_classes_ref.add_compiled_class(class_hash2, casm);
    pending_classes_ref.add_class(class_hash1, ApiContractClass::DeprecatedContractClass(class1));
    pending_classes_ref
        .add_class(*ACCOUNT_CLASS_HASH, ApiContractClass::DeprecatedContractClass(account_class));
    pending_classes_ref.add_class(
        *TEST_ERC20_CONTRACT_CLASS_HASH,
        ApiContractClass::DeprecatedContractClass(fee_contract_class),
    );

    *pending_data.write().await = PendingData {
        block: PendingBlock {
            eth_l1_gas_price: *GAS_PRICE,
            sequencer_address: *SEQUENCER_ADDRESS,
            timestamp: *BLOCK_TIMESTAMP,
            ..Default::default()
        },
        state_update: PendingStateUpdate {
            old_root: Default::default(),
            state_diff: ClientStateDiff {
                deployed_contracts: vec![
                    DeployedContract {
                        address: *DEPRECATED_CONTRACT_ADDRESS,
                        class_hash: class_hash1,
                    },
                    DeployedContract { address: *CONTRACT_ADDRESS, class_hash: class_hash2 },
                    DeployedContract { address: *ACCOUNT_ADDRESS, class_hash: *ACCOUNT_CLASS_HASH },
                    DeployedContract {
                        address: *TEST_ERC20_CONTRACT_ADDRESS,
                        class_hash: *TEST_ERC20_CONTRACT_CLASS_HASH,
                    },
                ],
                storage_diffs: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => vec![
                        // Give the accounts some balance.
                        StorageEntry {
                            key: account_balance_key, value: *ACCOUNT_INITIAL_BALANCE
                        },
                        // Give the first account mint permission (what is this?).
                        StorageEntry {
                            key: minter_var_address, value: ACCOUNT_ADDRESS.0.to_felt()
                        },
                    ],
                ),
                declared_classes: vec![DeclaredClassHashEntry {
                    class_hash: class_hash2,
                    compiled_class_hash,
                }],
                old_declared_contracts: vec![
                    class_hash1,
                    *ACCOUNT_CLASS_HASH,
                    *TEST_ERC20_CONTRACT_CLASS_HASH,
                ],
                nonces: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => Nonce::default(),
                    *CONTRACT_ADDRESS => Nonce::default(),
                    *DEPRECATED_CONTRACT_ADDRESS => Nonce::default(),
                    *ACCOUNT_ADDRESS => Nonce::default(),
                ),
                replaced_classes: vec![],
            },
        },
    }
}

fn prepare_storage_for_execution(mut storage_writer: StorageWriter) -> StorageWriter {
    let class1 = serde_json::from_value::<SN_API_DeprecatedContractClass>(read_json_file(
        "deprecated_class.json",
    ))
    .unwrap();
    let class_hash1 = class_hash!(0x1);

    let class2 = starknet_api::state::ContractClass::default();
    let casm = serde_json::from_value::<CasmContractClass>(read_json_file("casm.json")).unwrap();
    let class_hash2 = class_hash!(0x2);
    let compiled_class_hash = CompiledClassHash(Felt::default());

    let account_class = serde_json::from_value(read_json_file("account_class.json")).unwrap();
    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[ACCOUNT_ADDRESS.0.to_felt()]);

    let fee_contract_class = serde_json::from_value::<SN_API_DeprecatedContractClass>(
        read_json_file("erc20_fee_contract_class.json"),
    )
    .unwrap();
    let minter_var_address = get_storage_var_address("permitted_minter", &[]);

    let different_gas_price = GasPrice(GAS_PRICE.0 + 100);

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                eth_l1_gas_price: *GAS_PRICE,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            StateDiff {
                deployed_contracts: indexmap!(
                    *DEPRECATED_CONTRACT_ADDRESS => class_hash1,
                    *CONTRACT_ADDRESS => class_hash2,
                    *ACCOUNT_ADDRESS => *ACCOUNT_CLASS_HASH,
                    *TEST_ERC20_CONTRACT_ADDRESS => *TEST_ERC20_CONTRACT_CLASS_HASH,
                ),
                storage_diffs: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => indexmap!(
                        // Give the accounts some balance.
                        account_balance_key => *ACCOUNT_INITIAL_BALANCE,
                        // Give the first account mint permission (what is this?).
                        minter_var_address => ACCOUNT_ADDRESS.0.to_felt()
                    ),
                ),
                declared_classes: indexmap!(
                    class_hash2 =>
                    (compiled_class_hash, class2)
                ),
                deprecated_declared_classes: indexmap!(
                    class_hash1 => class1,
                    *ACCOUNT_CLASS_HASH => account_class,
                    *TEST_ERC20_CONTRACT_CLASS_HASH => fee_contract_class,
                ),
                nonces: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => Nonce::default(),
                    *CONTRACT_ADDRESS => Nonce::default(),
                    *DEPRECATED_CONTRACT_ADDRESS => Nonce::default(),
                    *ACCOUNT_ADDRESS => Nonce::default(),
                ),
                replaced_classes: indexmap!(),
            },
            indexmap!(),
        )
        .unwrap()
        .append_casm(&class_hash2, &casm)
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader {
                eth_l1_gas_price: different_gas_price,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                block_hash: BlockHash(Felt::ONE),
                block_number: BlockNumber(1),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(1), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(1), StateDiff::default(), indexmap![])
        .unwrap()
        .commit()
        .unwrap();

    storage_writer
}

fn write_empty_block(mut storage_writer: StorageWriter) {
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                eth_l1_gas_price: *GAS_PRICE,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(0), StateDiff::default(), indexmap!())
        .unwrap()
        .commit()
        .unwrap();
}

auto_impl_get_test_instance! {
    pub enum TransactionTrace {
        Invoke(InvokeTransactionTrace) = 0,
        Declare(DeclareTransactionTrace) = 1,
        DeployAccount(DeployAccountTransactionTrace) = 2,
    }
    pub struct InvokeTransactionTrace {
        pub validate_invocation: Option<FunctionInvocation>,
        pub execute_invocation: FunctionInvocationResult,
        pub fee_transfer_invocation: Option<FunctionInvocation>,
    }
    pub struct DeclareTransactionTrace {
        pub validate_invocation: Option<FunctionInvocation>,
        pub fee_transfer_invocation: Option<FunctionInvocation>,
    }
    pub struct DeployAccountTransactionTrace {
        pub validate_invocation: Option<FunctionInvocation>,
        pub constructor_invocation: FunctionInvocation,
        pub fee_transfer_invocation: Option<FunctionInvocation>,
    }
    pub struct L1HandlerTransactionTrace {
        pub function_invocation: FunctionInvocation,
    }
    pub enum FunctionInvocationResult {
        Ok(FunctionInvocation) = 0,
        Err(RevertReason) = 1,
    }
}

impl GetTestInstance for FunctionInvocation {
    fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
        Self {
            function_call: FunctionCall::get_test_instance(rng),
            caller_address: ContractAddress::get_test_instance(rng),
            class_hash: ClassHash::get_test_instance(rng),
            entry_point_type: EntryPointType::get_test_instance(rng),
            call_type: CallType::get_test_instance(rng),
            result: Retdata::get_test_instance(rng),
            calls: Vec::new(),
            events: Vec::<EventContent>::get_test_instance(rng),
            messages: Vec::<MessageToL1>::get_test_instance(rng),
        }
    }
}
