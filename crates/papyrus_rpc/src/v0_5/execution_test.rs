use std::env;
use std::fs::read_to_string;
use std::path::Path;

use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::{indexmap, IndexMap};
use jsonrpsee::core::Error;
use lazy_static::lazy_static;
use papyrus_execution::execution_utils::selector_from_name;
use papyrus_execution::objects::{
    DeclareTransactionTrace,
    DeployAccountTransactionTrace,
    FunctionInvocationResult,
    InvokeTransactionTrace,
    L1HandlerTransactionTrace,
    TransactionTrace,
};
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
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as SN_API_DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::StateDiff;
use starknet_api::transaction::{Calldata, Fee, TransactionHash, TransactionVersion};
use starknet_api::{calldata, class_hash, contract_address, patricia_key, stark_felt};
use test_utils::{auto_impl_get_test_instance, get_rng, read_json_file, GetTestInstance};

use super::api::api_impl::JsonRpcServerV0_5Impl as JsonRpcServerImpl;
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
use super::transaction::{DeployAccountTransaction, InvokeTransaction, InvokeTransactionV1};
use crate::api::{BlockHashOrNumber, BlockId};
use crate::test_utils::{
    get_starknet_spec_api_schema_for_components,
    get_starknet_spec_api_schema_for_method_results,
    get_test_rpc_server_and_storage_writer,
    validate_schema,
    SpecFile,
};
use crate::version_config::VERSION_0_5;

lazy_static! {
    pub static ref GAS_PRICE: GasPrice = GasPrice(100 * u128::pow(10, 9)); // Given in units of wei.
    pub static ref MAX_FEE: Fee = Fee(1000000 * GAS_PRICE.0);
    pub static ref BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234);
    pub static ref SEQUENCER_ADDRESS: ContractAddress = contract_address!("0xa");
    pub static ref DEPRECATED_CONTRACT_ADDRESS: ContractAddress = contract_address!("0x1");
    pub static ref CONTRACT_ADDRESS: ContractAddress = contract_address!("0x2");
    pub static ref ACCOUNT_CLASS_HASH: ClassHash = class_hash!("0x333");
    pub static ref ACCOUNT_ADDRESS: ContractAddress = contract_address!("0x444");
    pub static ref TEST_ERC20_CONTRACT_CLASS_HASH: ClassHash = class_hash!("0x1010");
    pub static ref TEST_ERC20_CONTRACT_ADDRESS: ContractAddress = contract_address!("0x1001");
    pub static ref ACCOUNT_INITIAL_BALANCE: StarkFelt = stark_felt!(2 * MAX_FEE.0);
}

#[tokio::test]
async fn execution_call() {
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    prepare_storage_for_execution(storage_writer);

    let key = stark_felt!(1234_u16);
    let value = stark_felt!(18_u8);

    let res = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_5_call",
            (
                *DEPRECATED_CONTRACT_ADDRESS.0.key(),
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
            "starknet_V0_5_call",
            (
                ContractAddress(patricia_key!("0x1234")),
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == CONTRACT_NOT_FOUND.into());

    // Calling a non-existent block.
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_5_call",
            (
                ContractAddress(patricia_key!("0x1234")),
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(7))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());

    // Calling a non-existent function (contract error).
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_5_call",
            (
                *DEPRECATED_CONTRACT_ADDRESS,
                selector_from_name("aaa"),
                calldata![key, value],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap_err();

    assert_matches!(err, Error::Call(err) if err == CONTRACT_ERROR.into());
}

#[tokio::test]
async fn call_estimate_fee() {
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    prepare_storage_for_execution(storage_writer);

    let account_address = ContractAddress(patricia_key!("0x444"));

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion::ONE,
        sender_address: account_address,
        calldata: calldata![
            *DEPRECATED_CONTRACT_ADDRESS.0.key(),  // Contract address.
            selector_from_name("return_result").0, // EP selector.
            stark_felt!(1_u8),                     // Calldata length.
            stark_felt!(2_u8)                      // Calldata: num.
        ],
        ..Default::default()
    }));

    let res = module
        .call::<_, Vec<FeeEstimate>>(
            "starknet_V0_5_estimateFee",
            (
                vec![invoke.clone()],
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
            ),
        )
        .await
        .unwrap();

    // TODO(yair): verify this is the correct fee, got this value by printing the result of the
    // call.
    let expected_fee_estimate = vec![FeeEstimate {
        gas_consumed: stark_felt!("0x9ba"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(249000000000000),
    }];

    assert_eq!(res, expected_fee_estimate);

    // Test that calling the same transaction with a different block context with a different gas
    // price produces a different fee.
    let res = module
        .call::<_, Vec<FeeEstimate>>(
            "starknet_V0_5_estimateFee",
            (vec![invoke], BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(1)))),
        )
        .await
        .unwrap();
    assert_ne!(res, expected_fee_estimate);
}

#[tokio::test]
async fn call_simulate() {
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    prepare_storage_for_execution(storage_writer);

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion::ONE,
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![
            *DEPRECATED_CONTRACT_ADDRESS.0.key(),  // Contract address.
            selector_from_name("return_result").0, // EP selector.
            stark_felt!(1_u8),                     // Calldata length.
            stark_felt!(2_u8)                      // Calldata: num.
        ],
        ..Default::default()
    }));

    let mut res = module
        .call::<_, Vec<SimulatedTransaction>>(
            "starknet_V0_5_simulateTransactions",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
                vec![invoke],
                Vec::<SimulationFlag>::new(),
            ),
        )
        .await
        .unwrap();

    assert_eq!(res.len(), 1);

    let simulated_tx = res.pop().unwrap();

    // TODO(yair): verify this is the correct fee, got this value by printing the result of the
    // call.
    // Why is it different from the estimate_fee call?
    let expected_fee_estimate = FeeEstimate {
        gas_consumed: stark_felt!("0x9ba"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(249000000000000),
    };

    assert_eq!(simulated_tx.fee_estimation, expected_fee_estimate);

    assert_matches!(simulated_tx.transaction_trace, TransactionTrace::Invoke(_));

    let TransactionTrace::Invoke(invoke_trace) = simulated_tx.transaction_trace else {
        unreachable!();
    };

    assert_matches!(invoke_trace.validate_invocation, Some(_));
    assert_matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_));
    assert_matches!(invoke_trace.fee_transfer_invocation, Some(_));
}

#[tokio::test]
async fn call_simulate_skip_validate() {
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    prepare_storage_for_execution(storage_writer);

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion::ONE,
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![
            *DEPRECATED_CONTRACT_ADDRESS.0.key(),  // Contract address.
            selector_from_name("return_result").0, // EP selector.
            stark_felt!(1_u8),                     // Calldata length.
            stark_felt!(2_u8)                      // Calldata: num.
        ],
        ..Default::default()
    }));

    let mut res = module
        .call::<_, Vec<SimulatedTransaction>>(
            "starknet_V0_5_simulateTransactions",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
                vec![invoke],
                vec![SimulationFlag::SkipValidate],
            ),
        )
        .await
        .unwrap();

    assert_eq!(res.len(), 1);

    let simulated_tx = res.pop().unwrap();

    // TODO(yair): verify this is the correct fee, got this value by printing the result of the
    // call.
    // Why is it different from the estimate_fee call?
    let expected_fee_estimate = FeeEstimate {
        gas_consumed: stark_felt!("0x9ba"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(249000000000000),
    };

    assert_eq!(simulated_tx.fee_estimation, expected_fee_estimate);

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
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    prepare_storage_for_execution(storage_writer);

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion::ONE,
        sender_address: *ACCOUNT_ADDRESS,
        calldata: calldata![
            *DEPRECATED_CONTRACT_ADDRESS.0.key(),  // Contract address.
            selector_from_name("return_result").0, // EP selector.
            stark_felt!(1_u8),                     // Calldata length.
            stark_felt!(2_u8)                      // Calldata: num.
        ],
        ..Default::default()
    }));

    let mut res = module
        .call::<_, Vec<SimulatedTransaction>>(
            "starknet_V0_5_simulateTransactions",
            (
                BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0))),
                vec![invoke],
                vec![SimulationFlag::SkipFeeCharge],
            ),
        )
        .await
        .unwrap();

    assert_eq!(res.len(), 1);

    let simulated_tx = res.pop().unwrap();

    // TODO(yair): verify this is the correct fee, got this value by printing the result of the
    // call.
    // Why is it different from the estimate_fee call?
    let expected_fee_estimate = FeeEstimate {
        gas_consumed: stark_felt!("9ba"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(249000000000000),
    };

    assert_eq!(simulated_tx.fee_estimation, expected_fee_estimate);

    assert_matches!(simulated_tx.transaction_trace, TransactionTrace::Invoke(_));

    let TransactionTrace::Invoke(invoke_trace) = simulated_tx.transaction_trace else {
        unreachable!();
    };

    assert_matches!(invoke_trace.validate_invocation, Some(_));
    assert_matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_));
    assert_matches!(invoke_trace.fee_transfer_invocation, None);
}

#[tokio::test]
async fn trace_block_transactions() {
    let (module, storage_writer) = get_test_rpc_server_and_storage_writer::<JsonRpcServerImpl>();

    let mut writer = prepare_storage_for_execution(storage_writer);

    let tx_hash1 = TransactionHash(stark_felt!("0x1234"));
    let tx_hash2 = TransactionHash(stark_felt!("0x5678"));

    let tx1 = starknet_api::transaction::Transaction::Invoke(
        starknet_api::transaction::InvokeTransaction::V1(
            starknet_api::transaction::InvokeTransactionV1 {
                max_fee: *MAX_FEE,
                sender_address: *ACCOUNT_ADDRESS,
                calldata: calldata![
                    *DEPRECATED_CONTRACT_ADDRESS.0.key(),  // Contract address.
                    selector_from_name("return_result").0, // EP selector.
                    stark_felt!(1_u8),                     // Calldata length.
                    stark_felt!(2_u8)                      // Calldata: num.
                ],
                nonce: Nonce(stark_felt!(0_u128)),
                ..Default::default()
            },
        ),
    );
    let tx2 = starknet_api::transaction::Transaction::Invoke(
        starknet_api::transaction::InvokeTransaction::V1(
            starknet_api::transaction::InvokeTransactionV1 {
                max_fee: *MAX_FEE,
                sender_address: *ACCOUNT_ADDRESS,
                calldata: calldata![
                    *DEPRECATED_CONTRACT_ADDRESS.0.key(),  // Contract address.
                    selector_from_name("return_result").0, // EP selector.
                    stark_felt!(1_u8),                     // Calldata length.
                    stark_felt!(2_u8)                      // Calldata: num.
                ],
                nonce: Nonce(stark_felt!(1_u128)),
                ..Default::default()
            },
        ),
    );
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(2),
            &BlockHeader {
                eth_l1_gas_price: *GAS_PRICE,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                block_hash: BlockHash(stark_felt!("0x2")),
                parent_hash: BlockHash(stark_felt!("0x1")),
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
        .append_state_diff(BlockNumber(2), StateDiff::default(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let tx_1_trace = module
        .call::<_, TransactionTrace>("starknet_V0_5_traceTransaction", [tx_hash1])
        .await
        .unwrap();

    assert_matches!(tx_1_trace, TransactionTrace::Invoke(_));

    let tx_2_trace = module
        .call::<_, TransactionTrace>("starknet_V0_5_traceTransaction", [tx_hash2])
        .await
        .unwrap();

    assert_matches!(tx_2_trace, TransactionTrace::Invoke(_));

    let res = module
        .call::<_, Vec<TransactionTraceWithHash>>(
            "starknet_V0_5_traceBlockTransactions",
            [BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(2)))],
        )
        .await
        .unwrap();

    assert_eq!(res.len(), 2);
    assert_eq!(res[0].trace_root, tx_1_trace);
    assert_eq!(res[0].transaction_hash, tx_hash1);
    assert_eq!(res[1].trace_root, tx_2_trace);
    assert_eq!(res[1].transaction_hash, tx_hash2);
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
        Ok(ExecutableTransactionInput::DeclareV1(_tx, _class))
    );
}

#[test]
fn validate_fee_estimation_schema() {
    let mut rng = get_rng();
    let fee_estimate = FeeEstimate::get_test_instance(&mut rng);
    let schema = get_starknet_spec_api_schema_for_components(
        &[(SpecFile::StarknetApiOpenrpc, &["FEE_ESTIMATE"])],
        &VERSION_0_5,
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
        &VERSION_0_5,
    );
    assert!(validate_schema(&schema, &serialized));
}

#[test]
fn validate_transaction_trace_schema() {
    let mut rng = get_rng();
    let schema = get_starknet_spec_api_schema_for_components(
        &[(SpecFile::TraceApi, &["TRANSACTION_TRACE"])],
        &VERSION_0_5,
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
        Ok(ExecutableTransactionInput::DeployAccount(_tx))
    );
}

#[test]
fn broadcasted_to_executable_invoke() {
    let mut rng = get_rng();
    let broadcasted_deploy_account =
        BroadcastedTransaction::Invoke(InvokeTransaction::get_test_instance(&mut rng));
    assert_matches!(
        broadcasted_deploy_account.try_into(),
        Ok(ExecutableTransactionInput::Invoke(_tx))
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
        pub gas_consumed: StarkFelt,
        pub gas_price: GasPrice,
        pub overall_fee: Fee,
    }

    pub struct TransactionTraceWithHash {
        pub transaction_hash: TransactionHash,
        pub trace_root: TransactionTrace,
    }
}

fn prepare_storage_for_execution(mut storage_writer: StorageWriter) -> StorageWriter {
    let class1 = serde_json::from_value::<SN_API_DeprecatedContractClass>(read_json_file(
        "deprecated_class.json",
    ))
    .unwrap();
    let class_hash1 = class_hash!("0x1");

    let class2 = starknet_api::state::ContractClass::default();
    let casm = serde_json::from_value::<CasmContractClass>(read_json_file("casm.json")).unwrap();
    let class_hash2 = class_hash!("0x2");
    let compiled_class_hash = CompiledClassHash(StarkHash::default());

    let account_class = serde_json::from_value(read_json_file("account_class.json")).unwrap();
    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[*ACCOUNT_ADDRESS.0.key()]);

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
                        minter_var_address => *ACCOUNT_ADDRESS.0.key()
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
                block_hash: BlockHash(stark_felt!("0x1")),
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
