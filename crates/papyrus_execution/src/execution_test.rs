use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::abi::constants::STEP_GAS_COST;
use blockifier::execution::call_info::Retdata;
use papyrus_storage::test_utils::get_test_storage;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ChainId, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::StateNumber;
use starknet_api::transaction::{Calldata, Fee};
use starknet_api::{calldata, contract_address, patricia_key, stark_felt};

use crate::execution_utils::selector_from_name;
use crate::objects::{
    DeclareTransactionTrace,
    DeployAccountTransactionTrace,
    FunctionInvocationResult,
    InvokeTransactionTrace,
    TransactionTrace,
};
use crate::test_utils::{
    execute_simulate_transactions,
    prepare_storage,
    TxsScenarioBuilder,
    ACCOUNT_ADDRESS,
    CHAIN_ID,
    CONTRACT_ADDRESS,
    DEPRECATED_CONTRACT_ADDRESS,
    GAS_PRICE,
    NEW_ACCOUNT_ADDRESS,
};
use crate::testing_instances::{test_block_execution_config, test_get_default_execution_config};
use crate::{
    estimate_fee,
    execute_call,
    BlockExecutionConfig,
    ExecutableTransactionInput,
    ExecutionConfigByBlock,
};

// Test calling entry points of a deprecated class.
#[test]
fn execute_call_cairo0() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let chain_id = ChainId(CHAIN_ID.to_string());

    // Test that the entry point can be called without arguments.

    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("without_arg"),
        Calldata::default(),
        &test_block_execution_config(),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata::default());

    // Test that the entry point can be called with arguments.
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("with_arg"),
        Calldata(Arc::new(vec![StarkFelt::from(25u128)])),
        &test_block_execution_config(),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata::default());

    // Test that the entry point can return a result.
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("return_result"),
        Calldata(Arc::new(vec![StarkFelt::from(123u128)])),
        &test_block_execution_config(),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata(vec![StarkFelt::from(123u128)]));

    // Test that the entry point can read and write to the contract storage.
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("test_storage_read_write"),
        Calldata(Arc::new(vec![StarkFelt::from(123u128), StarkFelt::from(456u128)])),
        &test_block_execution_config(),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata(vec![StarkFelt::from(456u128)]));
}

// Test calling entry points of a cairo 1 class.
#[test]
fn execute_call_cairo1() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let key = stark_felt!(1234_u16);
    let value = stark_felt!(18_u8);
    let calldata = calldata![key, value];

    // Test that the entry point can read and write to the contract storage.
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &CHAIN_ID,
        StateNumber::right_after_block(BlockNumber(0)),
        &CONTRACT_ADDRESS,
        selector_from_name("test_storage_read_write"),
        calldata,
        &test_block_execution_config(),
    )
    .unwrap()
    .retdata;

    assert_eq!(retdata, Retdata(vec![value]));
}

// TODO(yair): Compare to the expected fee instead of asserting that it is not zero (all
// estimate_fee tests).
#[ignore = "need to pass tx hashes"]
#[test]
fn estimate_fee_invoke() {
    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None)
        .collect();
    let fees = estimate_fees(tx);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[ignore = "need to pass tx hashes"]
#[test]
fn estimate_fee_declare_deprecated_class() {
    let tx = TxsScenarioBuilder::default().declare_deprecated_class(*ACCOUNT_ADDRESS).collect();

    let fees = estimate_fees(tx);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[ignore = "need to pass tx hashes"]
#[test]
fn estimate_fee_declare_class() {
    let tx = TxsScenarioBuilder::default().declare_class(*ACCOUNT_ADDRESS).collect();

    let fees = estimate_fees(tx);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[ignore = "need to pass tx hashes"]
#[test]
fn estimate_fee_deploy_account() {
    let tx = TxsScenarioBuilder::default().deploy_account().collect();

    let fees = estimate_fees(tx);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[ignore = "need to pass tx hashes"]
#[test]
fn estimate_fee_combination() {
    let txs = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None)
        .declare_class(*ACCOUNT_ADDRESS)
        .declare_deprecated_class(*ACCOUNT_ADDRESS)
        .deploy_account()
        .collect();

    let fees = estimate_fees(txs);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

fn estimate_fees(txs: Vec<ExecutableTransactionInput>) -> Vec<(GasPrice, Fee)> {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let storage_txn = storage_reader.begin_ro_txn().unwrap();

    estimate_fee(
        txs,
        &CHAIN_ID,
        &storage_txn,
        StateNumber::right_after_block(BlockNumber(0)),
        &test_block_execution_config(),
    )
    .unwrap()
}

#[test]
fn serialization_precision() {
    let input =
        "{\"value\":244116128358498188146337218061232635775543270890529169229936851982759783745}";
    let serialized = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let deserialized = serde_json::to_string(&serialized).unwrap();
    assert_eq!(input, deserialized);
}

#[ignore = "need to pass tx hashes"]
#[test]
fn simulate_invoke() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None)
        .collect();
    let exec_only_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, false, false);
    let validate_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, false, true);
    let charge_fee_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(&storage_reader, tx, None, true, true);

    for (exec_only, (validate, (charge_fee, charge_fee_validate))) in exec_only_results.iter().zip(
        validate_results
            .iter()
            .zip(charge_fee_results.iter().zip(charge_fee_validate_results.iter())),
    ) {
        let TransactionTrace::Invoke(exec_only_trace) = &exec_only.0 else {
            panic!("Wrong trace type, expected InvokeTransactionTrace.")
        };
        assert_matches!(
            exec_only_trace,
            InvokeTransactionTrace {
                validate_invocation: None,
                execute_invocation: FunctionInvocationResult::Ok(_),
                fee_transfer_invocation: None,
            }
        );

        let TransactionTrace::Invoke(validate_trace) = &validate.0 else {
            panic!("Wrong trace type, expected InvokeTransactionTrace.")
        };
        assert_matches!(
            validate_trace,
            InvokeTransactionTrace {
                validate_invocation: Some(_),
                execute_invocation: FunctionInvocationResult::Ok(_),
                fee_transfer_invocation: None,
            }
        );

        let TransactionTrace::Invoke(charge_fee_trace) = &charge_fee.0 else {
            panic!("Wrong trace type, expected InvokeTransactionTrace.")
        };
        assert_matches!(
            charge_fee_trace,
            InvokeTransactionTrace {
                validate_invocation: None,
                execute_invocation: FunctionInvocationResult::Ok(_),
                fee_transfer_invocation: Some(_),
            }
        );
        assert_eq!(charge_fee.1, *GAS_PRICE);

        assert_eq!(exec_only_trace.execute_invocation, charge_fee_trace.execute_invocation);

        let TransactionTrace::Invoke(charge_fee_validate_trace) = &charge_fee_validate.0 else {
            panic!("Wrong trace type, expected InvokeTransactionTrace.")
        };
        assert_matches!(
            charge_fee_validate_trace,
            InvokeTransactionTrace {
                validate_invocation: Some(_),
                execute_invocation: FunctionInvocationResult::Ok(_),
                fee_transfer_invocation: Some(_),
            }
        );

        // TODO(yair): Compare the trace to an expected trace.
    }
}

#[ignore = "need to pass tx hashes"]
#[test]
fn simulate_declare_deprecated() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default().declare_deprecated_class(*ACCOUNT_ADDRESS).collect();
    let exec_only_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, false, false);
    let validate_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, false, true);
    let charge_fee_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(&storage_reader, tx, None, true, true);

    for (exec_only, (validate, (charge_fee, charge_fee_validate))) in exec_only_results.iter().zip(
        validate_results
            .iter()
            .zip(charge_fee_results.iter().zip(charge_fee_validate_results.iter())),
    ) {
        let TransactionTrace::Declare(exec_only_trace) = &exec_only.0 else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            exec_only_trace,
            DeclareTransactionTrace { validate_invocation: None, fee_transfer_invocation: None }
        );

        let TransactionTrace::Declare(validate_trace) = &validate.0 else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            validate_trace,
            DeclareTransactionTrace { validate_invocation: Some(_), fee_transfer_invocation: None }
        );

        let TransactionTrace::Declare(charge_fee_trace) = &charge_fee.0 else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            charge_fee_trace,
            DeclareTransactionTrace { validate_invocation: None, fee_transfer_invocation: Some(_) }
        );

        let TransactionTrace::Declare(charge_fee_validate_trace) = &charge_fee_validate.0 else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            charge_fee_validate_trace,
            DeclareTransactionTrace {
                validate_invocation: Some(_),
                fee_transfer_invocation: Some(_),
            }
        );

        // TODO(yair): Compare the trace to an expected trace.
    }
}

#[ignore = "need to pass tx hashes"]
#[test]
fn simulate_declare() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default().declare_class(*ACCOUNT_ADDRESS).collect();
    let exec_only_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, false, false);
    let validate_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, false, true);
    let charge_fee_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(&storage_reader, tx, None, true, true);

    for (exec_only, (validate, (charge_fee, charge_fee_validate))) in exec_only_results.iter().zip(
        validate_results
            .iter()
            .zip(charge_fee_results.iter().zip(charge_fee_validate_results.iter())),
    ) {
        let TransactionTrace::Declare(exec_only_trace) = &exec_only.0 else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            exec_only_trace,
            DeclareTransactionTrace { validate_invocation: None, fee_transfer_invocation: None }
        );

        let TransactionTrace::Declare(validate_trace) = &validate.0 else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            validate_trace,
            DeclareTransactionTrace { validate_invocation: Some(_), fee_transfer_invocation: None }
        );

        let TransactionTrace::Declare(charge_fee_trace) = &charge_fee.0 else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            charge_fee_trace,
            DeclareTransactionTrace { validate_invocation: None, fee_transfer_invocation: Some(_) }
        );

        let TransactionTrace::Declare(charge_fee_validate_trace) = &charge_fee_validate.0 else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            charge_fee_validate_trace,
            DeclareTransactionTrace {
                validate_invocation: Some(_),
                fee_transfer_invocation: Some(_),
            }
        );

        // TODO(yair): Compare the trace to an expected trace.
    }
}

#[ignore = "need to pass tx hashes"]
#[test]
fn simulate_deploy_account() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default().deploy_account().collect();
    let exec_only_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, false, false);
    let validate_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, false, true);
    let charge_fee_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), None, true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(&storage_reader, tx, None, true, true);

    for (exec_only, (validate, (charge_fee, charge_fee_validate))) in exec_only_results.iter().zip(
        validate_results
            .iter()
            .zip(charge_fee_results.iter().zip(charge_fee_validate_results.iter())),
    ) {
        let TransactionTrace::DeployAccount(exec_only_trace) = &exec_only.0 else {
            panic!("Wrong trace type, expected DeployAccountTransactionTrace.")
        };
        assert_matches!(
            exec_only_trace,
            DeployAccountTransactionTrace {
                validate_invocation: None,
                fee_transfer_invocation: None,
                constructor_invocation: _,
            }
        );

        let TransactionTrace::DeployAccount(validate_trace) = &validate.0 else {
            panic!("Wrong trace type, expected DeployAccountTransactionTrace.")
        };
        assert_matches!(
            validate_trace,
            DeployAccountTransactionTrace {
                validate_invocation: Some(_),
                fee_transfer_invocation: None,
                constructor_invocation: _
            }
        );

        let TransactionTrace::DeployAccount(charge_fee_trace) = &charge_fee.0 else {
            panic!("Wrong trace type, expected DeployAccountTransactionTrace.")
        };
        assert_matches!(
            charge_fee_trace,
            DeployAccountTransactionTrace {
                validate_invocation: None,
                fee_transfer_invocation: Some(_),
                constructor_invocation: _
            }
        );

        let TransactionTrace::DeployAccount(charge_fee_validate_trace) = &charge_fee_validate.0
        else {
            panic!("Wrong trace type, expected DeployAccountTransactionTrace.")
        };
        assert_matches!(
            charge_fee_validate_trace,
            DeployAccountTransactionTrace {
                validate_invocation: Some(_),
                fee_transfer_invocation: Some(_),
                constructor_invocation: _
            }
        );

        // TODO(yair): Compare the trace to an expected trace.
    }
}

#[ignore = "need to pass tx hashes"]
#[test]
fn simulate_invoke_from_new_account() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let txs = TxsScenarioBuilder::default()
        // Invoke contract from a newly deployed account.
        .deploy_account()
        .invoke_deprecated(
            *NEW_ACCOUNT_ADDRESS,
            *DEPRECATED_CONTRACT_ADDRESS,
            // the deploy account make the next nonce be 1.
            Some(Nonce(stark_felt!(1_u128)))
        )
        // TODO(yair): Find out how to deploy another contract to test calling a new contract.
        .collect();

    let mut result = execute_simulate_transactions(&storage_reader, txs, None, false, false);
    assert_eq!(result.len(), 2);

    let Some((TransactionTrace::Invoke(invoke_trace), _, _)) = result.pop() else {
        panic!("Wrong trace type, expected InvokeTransactionTrace.")
    };
    let Some((TransactionTrace::DeployAccount(deploy_account_trace), _, _)) = result.pop() else {
        panic!("Wrong trace type, expected DeployAccountTransactionTrace.")
    };

    assert_eq!(
        deploy_account_trace.constructor_invocation.function_call.contract_address,
        *NEW_ACCOUNT_ADDRESS
    );

    // Check that the invoke transaction succeeded.
    assert_matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_));
}

#[ignore = "need to pass tx hashes"]
#[test]
fn simulate_invoke_from_new_account_validate_and_charge() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    // Taken from the trace of the deploy account transaction.
    let new_account_address = ContractAddress(patricia_key!(
        "0x0153ade9ef510502c4f3b879c049dcc3ad5866706cae665f0d9df9b01e794fdb"
    ));
    let txs = TxsScenarioBuilder::default()
        // Invoke contract from a newly deployed account.
        .deploy_account()
        .invoke_deprecated(
            new_account_address,
            *DEPRECATED_CONTRACT_ADDRESS,
            // the deploy account make the next nonce be 1.
            Some(Nonce(stark_felt!(1_u128)))
        )
        // TODO(yair): Find out how to deploy another contract to test calling a new contract.
        .collect();

    let mut result = execute_simulate_transactions(&storage_reader, txs, None, true, true);
    assert_eq!(result.len(), 2);

    let Some((TransactionTrace::Invoke(invoke_trace), _, invoke_fee_estimation)) = result.pop()
    else {
        panic!("Wrong trace type, expected InvokeTransactionTrace.")
    };
    let Some((TransactionTrace::DeployAccount(deploy_account_trace), _, deploy_fee_estimation)) =
        result.pop()
    else {
        panic!("Wrong trace type, expected DeployAccountTransactionTrace.")
    };

    assert_eq!(
        deploy_account_trace.constructor_invocation.function_call.contract_address,
        new_account_address
    );

    // Check that the invoke transaction succeeded.
    assert_matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_));

    // Check that the fee was charged.
    assert_ne!(deploy_fee_estimation, Fee(0));
    assert_matches!(deploy_account_trace.fee_transfer_invocation, Some(_));
    assert_ne!(invoke_fee_estimation, Fee(0));
    assert_matches!(invoke_trace.fee_transfer_invocation, Some(_));
}

/// Test that the execution config is loaded correctly. Compare the loaded config to the expected.
#[test]
fn test_default_execution_config() {
    let mut vm_resource_fee_cost = HashMap::new();
    vm_resource_fee_cost.insert("n_steps".to_owned(), 0.01);
    vm_resource_fee_cost.insert("pedersen_builtin".to_owned(), 0.32);
    vm_resource_fee_cost.insert("range_check_builtin".to_owned(), 0.16);
    vm_resource_fee_cost.insert("ecdsa_builtin".to_owned(), 20.48);
    vm_resource_fee_cost.insert("bitwise_builtin".to_owned(), 0.64);
    vm_resource_fee_cost.insert("poseidon_builtin".to_owned(), 0.32);
    vm_resource_fee_cost.insert("output_builtin".to_owned(), 1.0);
    vm_resource_fee_cost.insert("ec_op_builtin".to_owned(), 10.24);
    vm_resource_fee_cost.insert("keccak_builtin".to_owned(), 20.48);

    let vm_resource_fee_cost = Arc::new(vm_resource_fee_cost);
    let block_execution_config = BlockExecutionConfig {
        fee_contract_address: contract_address!(
            "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
        ),
        invoke_tx_max_n_steps: 1_000_000,
        validate_tx_max_n_steps: 1_000_000,
        max_recursion_depth: 50,
        step_gas_cost: STEP_GAS_COST,
        initial_gas_cost: 10_u64.pow(8) * STEP_GAS_COST,
        vm_resource_fee_cost,
    };
    let mut execution_config_segments = BTreeMap::new();
    execution_config_segments.insert(BlockNumber(0), block_execution_config);
    let expected_config = ExecutionConfigByBlock { execution_config_segments };
    let config_from_file = test_get_default_execution_config();
    assert_eq!(expected_config, config_from_file);
}

// TODO(Omri): Test loading of configuration according to the given block number.
