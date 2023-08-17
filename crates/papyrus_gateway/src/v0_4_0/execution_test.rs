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

use super::api::{decompress_program, FeeEstimate};
use super::broadcasted_transaction::{
    BroadcastedDeclareTransaction,
    BroadcastedDeclareV1Transaction,
    BroadcastedTransaction,
};
use super::transaction::{DeployAccountTransaction, InvokeTransactionV1};
use crate::api::{BlockHashOrNumber, BlockId};
use crate::test_utils::{
    get_starknet_spec_api_schema_for_components,
    get_test_rpc_server_and_storage_writer,
    validate_schema,
    SpecFile,
};
use crate::v0_4_0::api::api_impl::JsonRpcServerV0_4Impl;
use crate::v0_4_0::api::{SimulatedTransaction, SimulationFlag};
use crate::v0_4_0::error::{BLOCK_NOT_FOUND, CONTRACT_ERROR, CONTRACT_NOT_FOUND};
use crate::v0_4_0::transaction::InvokeTransaction;
use crate::version_config::VERSION_0_4;

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
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let key = stark_felt!(1234_u16);
    let value = stark_felt!(18_u8);

    let res = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
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

    assert_matches!(err, Error::Call(err) if err == CONTRACT_NOT_FOUND.into());

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

    assert_matches!(err, Error::Call(err) if err == BLOCK_NOT_FOUND.into());

    // Calling a non-existent function (contract error).
    let err = module
        .call::<_, Vec<StarkFelt>>(
            "starknet_V0_4_call",
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
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let account_address = ContractAddress(patricia_key!("0x444"));

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion(stark_felt!("0x1")),
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
            "starknet_V0_4_estimateFee",
            (vec![invoke], BlockId::HashOrNumber(BlockHashOrNumber::Number(BlockNumber(0)))),
        )
        .await
        .unwrap();

    // TODO(yair): verify this is the correct fee, got this value by printing the result of the
    // call.
    let expected_fee_estimate = vec![FeeEstimate {
        gas_consumed: stark_felt!("0x19a2"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(656200000000000),
    }];

    assert_eq!(res, expected_fee_estimate);
}

#[tokio::test]
async fn call_simulate() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion(stark_felt!("0x1")),
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
            "starknet_V0_4_simulateTransactions",
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
        gas_consumed: stark_felt!("0x19b7"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(658300000000000),
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
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion(stark_felt!("0x1")),
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
            "starknet_V0_4_simulateTransactions",
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
        gas_consumed: stark_felt!("0x19a2"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(656200000000000),
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
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    prepare_storage_for_execution(storage_writer);

    let invoke = BroadcastedTransaction::Invoke(InvokeTransaction::Version1(InvokeTransactionV1 {
        max_fee: Fee(1000000 * GAS_PRICE.0),
        version: TransactionVersion(stark_felt!("0x1")),
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
            "starknet_V0_4_simulateTransactions",
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
        gas_consumed: stark_felt!("0x19b7"),
        gas_price: *GAS_PRICE,
        overall_fee: Fee(658300000000000),
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
async fn call_trace_transaction() {
    let (module, storage_writer) =
        get_test_rpc_server_and_storage_writer::<JsonRpcServerV0_4Impl>();

    let mut writer = prepare_storage_for_execution(storage_writer);

    let tx_hash = TransactionHash(stark_felt!("0x1234"));
    writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader {
                gas_price: *GAS_PRICE,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                block_hash: BlockHash(stark_felt!("0x1")),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(
            BlockNumber(1),
            BlockBody {
                transactions: vec![starknet_api::transaction::Transaction::Invoke(
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
                            ..Default::default()
                        },
                    ),
                )],
                transaction_outputs: vec![starknet_api::transaction::TransactionOutput::Invoke(
                    starknet_api::transaction::InvokeTransactionOutput::default(),
                )],
                transaction_hashes: vec![tx_hash],
            },
        )
        .unwrap()
        .append_state_diff(BlockNumber(1), StateDiff::default(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let res = module
        .call::<_, TransactionTrace>("starknet_V0_4_traceTransaction", [tx_hash])
        .await
        .unwrap();

    assert_matches!(res, TransactionTrace::Invoke(_));
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
        &VERSION_0_4,
    );
    let serialized = serde_json::to_value(fee_estimate).unwrap();
    assert!(validate_schema(&schema, &serialized));
}

#[test]
fn validate_transaction_trace_schema() {
    let mut rng = get_rng();
    let schema = get_starknet_spec_api_schema_for_components(
        &[(SpecFile::StarknetTraceApi, &["TRANSACTION_TRACE"])],
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
}

#[test]
fn broadcasted_to_executable_deploy_account() {
    let mut rng = get_rng();
    let broadcasted_deploy_account = BroadcastedTransaction::DeployAccount(
        DeployAccountTransaction::get_test_instance(&mut rng),
    );
    assert_matches!(
        broadcasted_deploy_account.try_into(),
        Ok(ExecutableTransactionInput::Deploy(_tx))
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
        get_storage_var_address("ERC20_balances", &[*ACCOUNT_ADDRESS.0.key()]).unwrap();

    let fee_contract_class = serde_json::from_value::<SN_API_DeprecatedContractClass>(
        read_json_file("erc20_fee_contract_class.json"),
    )
    .unwrap();
    let minter_var_address = get_storage_var_address("permitted_minter", &[])
        .expect("Failed to get permitted_minter storage address.");

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                gas_price: *GAS_PRICE,
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
        .commit()
        .unwrap();

    storage_writer
}
