// TODO(shahak): Add a test for executing when there's a missing casm that's not required and when
// there's a missing casm that is required.
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::abi::abi_utils::get_storage_var_address;
use blockifier::abi::constants::STEP_GAS_COST;
use blockifier::execution::call_info::Retdata;
use blockifier::transaction::errors::TransactionExecutionError as BlockifierTransactionExecutionError;
use indexmap::indexmap;
use num_traits::ToPrimitive;
use papyrus_storage::test_utils::get_test_storage;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::{
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::state::{StateNumber, ThinStateDiff};
use starknet_api::transaction::{Calldata, Fee};
use starknet_api::{calldata, class_hash, contract_address, patricia_key};
use starknet_types_core::felt::Felt;

use crate::execution_utils::selector_from_name;
use crate::objects::{
    DeclareTransactionTrace,
    DeployAccountTransactionTrace,
    FunctionInvocationResult,
    InvokeTransactionTrace,
    PriceUnit,
    TransactionSimulationOutput,
    TransactionTrace,
};
use crate::test_utils::{
    execute_simulate_transactions,
    prepare_storage,
    TxsScenarioBuilder,
    ACCOUNT_ADDRESS,
    ACCOUNT_CLASS_HASH,
    ACCOUNT_INITIAL_BALANCE,
    CHAIN_ID,
    CONTRACT_ADDRESS,
    DEPRECATED_CONTRACT_ADDRESS,
    GAS_PRICE,
    NEW_ACCOUNT_ADDRESS,
    SEQUENCER_ADDRESS,
    TEST_ERC20_CONTRACT_ADDRESS,
};
use crate::testing_instances::{test_block_execution_config, test_get_default_execution_config};
use crate::{
    estimate_fee,
    execute_call,
    BlockExecutionConfig,
    ExecutableTransactionInput,
    ExecutionConfigByBlock,
    ExecutionError,
    FeeEstimationResult,
    RevertedTransaction,
};

// Test calling entry points of a deprecated class.
#[test]
fn execute_call_cairo0() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let chain_id = ChainId(CHAIN_ID.to_string());

    // Test that the entry point can be called without arguments.

    let retdata = execute_call(
        storage_reader.clone(),
        None,
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        BlockNumber(0),
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
        storage_reader.clone(),
        None,
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        BlockNumber(0),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("with_arg"),
        Calldata(Arc::new(vec![Felt::from(25u128)])),
        &test_block_execution_config(),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata::default());

    // Test that the entry point can return a result.
    let retdata = execute_call(
        storage_reader.clone(),
        None,
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        BlockNumber(0),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("return_result"),
        Calldata(Arc::new(vec![Felt::from(123u128)])),
        &test_block_execution_config(),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata(vec![Felt::from(123u128)]));

    // Test that the entry point can read and write to the contract storage.
    let retdata = execute_call(
        storage_reader,
        None,
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        BlockNumber(0),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("test_storage_read_write"),
        Calldata(Arc::new(vec![Felt::from(123u128), Felt::from(456u128)])),
        &test_block_execution_config(),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata(vec![Felt::from(456u128)]));
}

// Test calling entry points of a cairo 1 class.
#[test]
fn execute_call_cairo1() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let key = Felt::from_hex_unchecked("0x1234");
    let value = Felt::from_hex_unchecked("0x18");
    let calldata = calldata![key, value];

    // Test that the entry point can read and write to the contract storage.
    let retdata = execute_call(
        storage_reader,
        None,
        &CHAIN_ID,
        StateNumber::right_after_block(BlockNumber(0)),
        BlockNumber(0),
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
#[test]
fn estimate_fee_invoke() {
    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false)
        .collect();
    let fees = estimate_fees(tx).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[test]
fn estimate_fee_declare_deprecated_class() {
    let tx = TxsScenarioBuilder::default().declare_deprecated_class(*ACCOUNT_ADDRESS).collect();

    let fees = estimate_fees(tx).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[test]
fn estimate_fee_declare_class() {
    let tx = TxsScenarioBuilder::default().declare_class(*ACCOUNT_ADDRESS).collect();

    let fees = estimate_fees(tx).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[test]
fn estimate_fee_deploy_account() {
    let tx = TxsScenarioBuilder::default().deploy_account().collect();

    let fees = estimate_fees(tx).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[test]
fn estimate_fee_combination() {
    let txs = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false)
        .declare_class(*ACCOUNT_ADDRESS)
        .declare_deprecated_class(*ACCOUNT_ADDRESS)
        .deploy_account()
        .collect();

    let fees = estimate_fees(txs).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, *GAS_PRICE);
    }
}

#[test]
fn estimate_fee_reverted() {
    let non_existing_contract = contract_address!(0x987);
    let txs = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false)
        .invoke_deprecated(*ACCOUNT_ADDRESS, non_existing_contract, None, false)
        .collect();

    let failed_estimation = estimate_fees(txs).expect_err("Fee estimation should fail.");
    assert_matches!(failed_estimation, RevertedTransaction { index: 1, revert_reason: _ })
}

fn estimate_fees(txs: Vec<ExecutableTransactionInput>) -> FeeEstimationResult {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    estimate_fee(
        txs,
        &CHAIN_ID,
        storage_reader,
        None,
        StateNumber::right_after_block(BlockNumber(0)),
        BlockNumber(1),
        &test_block_execution_config(),
        false,
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

#[test]
fn simulate_invoke() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false)
        .collect();
    let exec_only_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, false, false);
    let validate_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, false, true);
    let charge_fee_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(storage_reader, None, tx, None, true, true);

    for (exec_only, (validate, (charge_fee, charge_fee_validate))) in exec_only_results.iter().zip(
        validate_results
            .iter()
            .zip(charge_fee_results.iter().zip(charge_fee_validate_results.iter())),
    ) {
        let TransactionTrace::Invoke(exec_only_trace) = &exec_only.transaction_trace else {
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

        let TransactionTrace::Invoke(validate_trace) = &validate.transaction_trace else {
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

        let TransactionTrace::Invoke(charge_fee_trace) = &charge_fee.transaction_trace else {
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
        assert_eq!(charge_fee.gas_price, *GAS_PRICE);

        assert_eq!(exec_only_trace.execute_invocation, charge_fee_trace.execute_invocation);

        let TransactionTrace::Invoke(charge_fee_validate_trace) =
            &charge_fee_validate.transaction_trace
        else {
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

#[test]
fn simulate_declare_deprecated() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default().declare_deprecated_class(*ACCOUNT_ADDRESS).collect();
    let exec_only_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, false, false);
    let validate_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, false, true);
    let charge_fee_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(storage_reader, None, tx, None, true, true);

    for (exec_only, (validate, (charge_fee, charge_fee_validate))) in exec_only_results.iter().zip(
        validate_results
            .iter()
            .zip(charge_fee_results.iter().zip(charge_fee_validate_results.iter())),
    ) {
        let TransactionTrace::Declare(exec_only_trace) = &exec_only.transaction_trace else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            exec_only_trace,
            DeclareTransactionTrace { validate_invocation: None, fee_transfer_invocation: None }
        );

        let TransactionTrace::Declare(validate_trace) = &validate.transaction_trace else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            validate_trace,
            DeclareTransactionTrace { validate_invocation: Some(_), fee_transfer_invocation: None }
        );

        let TransactionTrace::Declare(charge_fee_trace) = &charge_fee.transaction_trace else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            charge_fee_trace,
            DeclareTransactionTrace { validate_invocation: None, fee_transfer_invocation: Some(_) }
        );

        let TransactionTrace::Declare(charge_fee_validate_trace) =
            &charge_fee_validate.transaction_trace
        else {
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

#[test]
fn simulate_declare() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default().declare_class(*ACCOUNT_ADDRESS).collect();
    let exec_only_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, false, false);
    let validate_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, false, true);
    let charge_fee_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(storage_reader, None, tx, None, true, true);

    for (exec_only, (validate, (charge_fee, charge_fee_validate))) in exec_only_results.iter().zip(
        validate_results
            .iter()
            .zip(charge_fee_results.iter().zip(charge_fee_validate_results.iter())),
    ) {
        let TransactionTrace::Declare(exec_only_trace) = &exec_only.transaction_trace else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            exec_only_trace,
            DeclareTransactionTrace { validate_invocation: None, fee_transfer_invocation: None }
        );

        let TransactionTrace::Declare(validate_trace) = &validate.transaction_trace else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            validate_trace,
            DeclareTransactionTrace { validate_invocation: Some(_), fee_transfer_invocation: None }
        );

        let TransactionTrace::Declare(charge_fee_trace) = &charge_fee.transaction_trace else {
            panic!("Wrong trace type, expected DeclareTransactionTrace.")
        };
        assert_matches!(
            charge_fee_trace,
            DeclareTransactionTrace { validate_invocation: None, fee_transfer_invocation: Some(_) }
        );

        let TransactionTrace::Declare(charge_fee_validate_trace) =
            &charge_fee_validate.transaction_trace
        else {
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

#[test]
fn simulate_deploy_account() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default().deploy_account().collect();
    let exec_only_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, false, false);
    let validate_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, false, true);
    let charge_fee_results =
        execute_simulate_transactions(storage_reader.clone(), None, tx.clone(), None, true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(storage_reader, None, tx, None, true, true);

    for (exec_only, (validate, (charge_fee, charge_fee_validate))) in exec_only_results.iter().zip(
        validate_results
            .iter()
            .zip(charge_fee_results.iter().zip(charge_fee_validate_results.iter())),
    ) {
        let TransactionTrace::DeployAccount(exec_only_trace) = &exec_only.transaction_trace else {
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

        let TransactionTrace::DeployAccount(validate_trace) = &validate.transaction_trace else {
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

        let TransactionTrace::DeployAccount(charge_fee_trace) = &charge_fee.transaction_trace
        else {
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

        let TransactionTrace::DeployAccount(charge_fee_validate_trace) =
            &charge_fee_validate.transaction_trace
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
            Some(Nonce(Felt::ONE)),
            false,
        )
        // TODO(yair): Find out how to deploy another contract to test calling a new contract.
        .collect();

    let mut result = execute_simulate_transactions(storage_reader, None, txs, None, false, false);
    assert_eq!(result.len(), 2);

    let Some(TransactionSimulationOutput {
        transaction_trace: TransactionTrace::Invoke(invoke_trace),
        ..
    }) = result.pop()
    else {
        panic!("Wrong trace type, expected InvokeTransactionTrace.")
    };
    let Some(TransactionSimulationOutput {
        transaction_trace: TransactionTrace::DeployAccount(deploy_account_trace),
        ..
    }) = result.pop()
    else {
        panic!("Wrong trace type, expected DeployAccountTransactionTrace.")
    };

    assert_eq!(
        deploy_account_trace.constructor_invocation.function_call.contract_address,
        *NEW_ACCOUNT_ADDRESS
    );

    // Check that the invoke transaction succeeded.
    assert_matches!(invoke_trace.execute_invocation, FunctionInvocationResult::Ok(_));
}

#[test]
fn simulate_invoke_from_new_account_validate_and_charge() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    // Taken from the trace of the deploy account transaction.
    let new_account_address = ContractAddress(
        PatriciaKey::try_from(
            Felt::from_hex("0x0153ade9ef510502c4f3b879c049dcc3ad5866706cae665f0d9df9b01e794fdb")
                .unwrap(),
        )
        .unwrap(),
    );
    let txs = TxsScenarioBuilder::default()
        // Invoke contract from a newly deployed account.
        .deploy_account()
        .invoke_deprecated(
            new_account_address,
            *DEPRECATED_CONTRACT_ADDRESS,
            // the deploy account make the next nonce be 1.
            Some(Nonce(Felt::ONE)),
            false,
        )
        // TODO(yair): Find out how to deploy another contract to test calling a new contract.
        .collect();

    let mut result = execute_simulate_transactions(storage_reader, None, txs, None, true, true);
    assert_eq!(result.len(), 2);

    let Some(TransactionSimulationOutput {
        transaction_trace: TransactionTrace::Invoke(invoke_trace),
        fee: invoke_fee_estimation,
        price_unit: invoke_unit,
        ..
    }) = result.pop()
    else {
        panic!("Wrong trace type, expected InvokeTransactionTrace.")
    };
    let Some(TransactionSimulationOutput {
        transaction_trace: TransactionTrace::DeployAccount(deploy_account_trace),
        fee: deploy_fee_estimation,
        price_unit: deploy_unit,
        ..
    }) = result.pop()
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
    assert_eq!(invoke_unit, PriceUnit::Wei);
    assert_matches!(deploy_account_trace.fee_transfer_invocation, Some(_));
    assert_ne!(invoke_fee_estimation, Fee(0));
    assert_eq!(deploy_unit, PriceUnit::Wei);
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
        fee_contract_address: ContractAddress(
            PatriciaKey::try_from(
                Felt::from_hex(
                    "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
                )
                .unwrap(),
            )
            .unwrap(),
        ),
        invoke_tx_max_n_steps: 3_000_000,
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

fn fill_up_block_execution_config_segment_with_value(value: usize) -> BlockExecutionConfig {
    let vm_resource_fee_cost = HashMap::new();
    let vm_resource_fee_cost = Arc::new(vm_resource_fee_cost);
    BlockExecutionConfig {
        fee_contract_address: contract_address!(value),
        invoke_tx_max_n_steps: value as u32,
        validate_tx_max_n_steps: value as u32,
        max_recursion_depth: value,
        step_gas_cost: value as u64,
        initial_gas_cost: value as u64,
        vm_resource_fee_cost,
    }
}

#[test]
/// Test for the get_execution_config_for_block function.
fn test_get_execution_config_for_block() {
    let mut execution_config_segments: BTreeMap<BlockNumber, BlockExecutionConfig> =
        BTreeMap::new();
    let segment_block_numbers = vec![0, 67, 1005, 20369];
    for block_number in segment_block_numbers {
        execution_config_segments.insert(
            BlockNumber(block_number as u64),
            fill_up_block_execution_config_segment_with_value(block_number),
        );
    }
    let execution_config_by_block = ExecutionConfigByBlock { execution_config_segments };

    assert_eq!(
        execution_config_by_block.get_execution_config_for_block(BlockNumber(0)).unwrap(),
        &fill_up_block_execution_config_segment_with_value(0),
        "Failed to get config for {:?}",
        BlockNumber(0),
    );
    assert_eq!(
        execution_config_by_block.get_execution_config_for_block(BlockNumber(67)).unwrap(),
        &fill_up_block_execution_config_segment_with_value(67),
        "Failed to get config for {:?}",
        BlockNumber(67),
    );
    assert_eq!(
        execution_config_by_block.get_execution_config_for_block(BlockNumber(517)).unwrap(),
        &fill_up_block_execution_config_segment_with_value(67),
        "Failed to get config for {:?}",
        BlockNumber(517),
    );
    assert_eq!(
        execution_config_by_block.get_execution_config_for_block(BlockNumber(20400)).unwrap(),
        &fill_up_block_execution_config_segment_with_value(20369),
        "Failed to get config for {:?}",
        BlockNumber(20400),
    );
}

#[test]
fn induced_state_diff() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);
    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[ACCOUNT_ADDRESS.0.to_felt()]);
    let sequencer_balance_key =
        get_storage_var_address("ERC20_balances", &[SEQUENCER_ADDRESS.0.to_felt()]);

    // TODO(yair): Add a reverted transaction.
    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false)
        .declare_class(*ACCOUNT_ADDRESS)
        .declare_deprecated_class(*ACCOUNT_ADDRESS)
        .deploy_account()
        .collect();
    let simulation_results =
        execute_simulate_transactions(storage_reader, None, tx, None, true, true);
    // This is the value TxsScenarioBuilder uses for the first declared class hash.
    let mut next_declared_class_hash = 100_u128;
    let mut account_balance = ACCOUNT_INITIAL_BALANCE.to_u64().unwrap() as u128;
    let mut sequencer_balance = 0_u128;

    account_balance -= simulation_results[0].fee.0;
    sequencer_balance += simulation_results[0].fee.0;
    let expected_invoke_deprecated = ThinStateDiff {
        nonces: indexmap! {*ACCOUNT_ADDRESS => Nonce(Felt::ONE)},
        deployed_contracts: indexmap! {},
        storage_diffs: indexmap! {
            *TEST_ERC20_CONTRACT_ADDRESS => indexmap!{
                account_balance_key => Felt::from(account_balance),
                sequencer_balance_key => Felt::from(sequencer_balance),
            },
        },
        declared_classes: indexmap! {},
        deprecated_declared_classes: vec![],
        replaced_classes: indexmap! {},
    };
    assert_eq!(simulation_results[0].induced_state_diff, expected_invoke_deprecated);

    account_balance -= simulation_results[1].fee.0;
    sequencer_balance += simulation_results[1].fee.0;
    let expected_declare_class = ThinStateDiff {
        nonces: indexmap! {*ACCOUNT_ADDRESS => Nonce(Felt::TWO)},
        declared_classes: indexmap! {class_hash!(next_declared_class_hash) => CompiledClassHash::default()},
        storage_diffs: indexmap! {
            *TEST_ERC20_CONTRACT_ADDRESS => indexmap!{
                account_balance_key => Felt::from(account_balance),
                sequencer_balance_key => Felt::from(sequencer_balance),
            },
        },
        deployed_contracts: indexmap! {},
        deprecated_declared_classes: vec![],
        replaced_classes: indexmap! {},
    };
    assert_eq!(simulation_results[1].induced_state_diff, expected_declare_class);
    next_declared_class_hash += 1;

    account_balance -= simulation_results[2].fee.0;
    sequencer_balance += simulation_results[2].fee.0;
    let expected_declare_deprecated_class = ThinStateDiff {
        nonces: indexmap! {*ACCOUNT_ADDRESS => Nonce(Felt::THREE)},
        deprecated_declared_classes: vec![class_hash!(next_declared_class_hash)],
        storage_diffs: indexmap! {
            *TEST_ERC20_CONTRACT_ADDRESS => indexmap!{
                account_balance_key => Felt::from(account_balance),
                sequencer_balance_key => Felt::from(sequencer_balance),
            },
        },
        declared_classes: indexmap! {},
        deployed_contracts: indexmap! {},
        replaced_classes: indexmap! {},
    };
    assert_eq!(simulation_results[2].induced_state_diff, expected_declare_deprecated_class);

    let new_account_balance_key =
        get_storage_var_address("ERC20_balances", &[NEW_ACCOUNT_ADDRESS.0.to_felt()]);
    let new_account_balance =
        ACCOUNT_INITIAL_BALANCE.to_u64().unwrap() as u128 - simulation_results[3].fee.0;

    sequencer_balance += simulation_results[3].fee.0;
    let expected_deploy_account = ThinStateDiff {
        nonces: indexmap! {*NEW_ACCOUNT_ADDRESS => Nonce(Felt::ONE)},
        deprecated_declared_classes: vec![],
        storage_diffs: indexmap! {
            *TEST_ERC20_CONTRACT_ADDRESS => indexmap!{
                new_account_balance_key => Felt::from(new_account_balance),
                sequencer_balance_key => Felt::from(sequencer_balance),
            },
        },
        declared_classes: indexmap! {},
        deployed_contracts: indexmap! {*NEW_ACCOUNT_ADDRESS => *ACCOUNT_CLASS_HASH},
        replaced_classes: indexmap! {},
    };
    assert_eq!(simulation_results[3].induced_state_diff, expected_deploy_account);
}

#[test]
fn simulate_with_query_bit_outputs_same_as_no_query_bit() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    // A tx with only_query=true.
    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, true)
        .collect();

    let res_only_query =
        execute_simulate_transactions(storage_reader.clone(), None, tx, None, false, false);

    // A tx with only_query=false.
    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false)
        .collect();

    let res_regular =
        execute_simulate_transactions(storage_reader.clone(), None, tx, None, false, false);

    assert_eq!(res_only_query, res_regular);
}

// Test that we provide the correct messages for different blockifier error variants.
// TODO(yair): remove once blockifier arranges the errors.
#[test]
fn blockifier_error_mapping() {
    let child = blockifier::execution::errors::EntryPointExecutionError::RecursionDepthExceeded;
    let expected = format!("Contract constructor execution has failed: {child}");
    let blockifier_err =
        BlockifierTransactionExecutionError::ContractConstructorExecutionFailed(child);
    let err = ExecutionError::from((0, blockifier_err));
    let ExecutionError::TransactionExecutionError { transaction_index, execution_error } = err
    else {
        panic!("unexpected variant")
    };
    assert_eq!(execution_error, expected);
    assert_eq!(transaction_index, 0);

    let child = blockifier::execution::errors::EntryPointExecutionError::RecursionDepthExceeded;
    let expected = format!("Transaction execution has failed: {child}");
    let blockifier_err = BlockifierTransactionExecutionError::ExecutionError(child);
    let err = ExecutionError::from((0, blockifier_err));
    let ExecutionError::TransactionExecutionError { transaction_index, execution_error } = err
    else {
        panic!("unexpected variant")
    };
    assert_eq!(execution_error, expected);
    assert_eq!(transaction_index, 0);

    let child = blockifier::execution::errors::EntryPointExecutionError::RecursionDepthExceeded;
    let expected = format!("Transaction validation has failed: {child}");
    let blockifier_err = BlockifierTransactionExecutionError::ValidateTransactionError(child);
    let err = ExecutionError::from((0, blockifier_err));
    let ExecutionError::TransactionExecutionError { transaction_index, execution_error } = err
    else {
        panic!("unexpected variant")
    };
    assert_eq!(execution_error, expected);
    assert_eq!(transaction_index, 0);
}
