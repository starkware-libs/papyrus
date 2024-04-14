// TODO(shahak): Add a test for executing when there's a missing casm that's not required and when
// there's a missing casm that is required.
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::abi::abi_utils::get_storage_var_address;
use blockifier::execution::call_info::Retdata;
use blockifier::execution::errors::gen_transaction_execution_error_trace;
use blockifier::transaction::errors::TransactionExecutionError as BlockifierTransactionExecutionError;
use indexmap::indexmap;
use papyrus_storage::test_utils::get_test_storage;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockNumber, StarknetVersion};
use starknet_api::core::{
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    Nonce,
    PatriciaKey,
};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{StateNumber, ThinStateDiff};
use starknet_api::transaction::{Calldata, Fee};
use starknet_api::{calldata, class_hash, contract_address, patricia_key, stark_felt};

use crate::execution_utils::selector_from_name;
use crate::objects::{
    DeclareTransactionTrace,
    DeployAccountTransactionTrace,
    FeeEstimation,
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
use crate::testing_instances::get_test_execution_config;
use crate::{
    estimate_fee,
    execute_call,
    get_versioned_constants,
    ExecutableTransactionInput,
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
        StateNumber::unchecked_right_after_block(BlockNumber(0)),
        BlockNumber(0),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("without_arg"),
        Calldata::default(),
        &get_test_execution_config(),
        true,
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata::default());

    // Test that the entry point can be called with arguments.
    let retdata = execute_call(
        storage_reader.clone(),
        None,
        &chain_id,
        StateNumber::unchecked_right_after_block(BlockNumber(0)),
        BlockNumber(0),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("with_arg"),
        Calldata(Arc::new(vec![StarkFelt::from(25u128)])),
        &get_test_execution_config(),
        true,
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata::default());

    // Test that the entry point can return a result.
    let retdata = execute_call(
        storage_reader.clone(),
        None,
        &chain_id,
        StateNumber::unchecked_right_after_block(BlockNumber(0)),
        BlockNumber(0),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("return_result"),
        Calldata(Arc::new(vec![StarkFelt::from(123u128)])),
        &get_test_execution_config(),
        true,
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata(vec![StarkFelt::from(123u128)]));

    // Test that the entry point can read and write to the contract storage.
    let retdata = execute_call(
        storage_reader,
        None,
        &chain_id,
        StateNumber::unchecked_right_after_block(BlockNumber(0)),
        BlockNumber(0),
        &DEPRECATED_CONTRACT_ADDRESS,
        selector_from_name("test_storage_read_write"),
        Calldata(Arc::new(vec![StarkFelt::from(123u128), StarkFelt::from(456u128)])),
        &get_test_execution_config(),
        true,
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
        storage_reader,
        None,
        &CHAIN_ID,
        StateNumber::unchecked_right_after_block(BlockNumber(0)),
        BlockNumber(0),
        &CONTRACT_ADDRESS,
        selector_from_name("test_storage_read_write"),
        calldata,
        &get_test_execution_config(),
        true,
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
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false, None)
        .collect();
    let fees = estimate_fees(tx).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.overall_fee, Fee(0));
        assert_eq!(fee.gas_price, GAS_PRICE.price_in_wei);
    }
}

#[test]
fn estimate_fee_declare_deprecated_class() {
    let tx = TxsScenarioBuilder::default().declare_deprecated_class(*ACCOUNT_ADDRESS).collect();

    let fees = estimate_fees(tx).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.overall_fee, Fee(0));
        assert_eq!(fee.gas_price, GAS_PRICE.price_in_wei);
    }
}

#[test]
fn estimate_fee_declare_class() {
    let tx = TxsScenarioBuilder::default().declare_class(*ACCOUNT_ADDRESS).collect();

    let fees = estimate_fees(tx).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.overall_fee, Fee(0));
        assert_eq!(fee.gas_price, GAS_PRICE.price_in_wei);
    }
}

#[test]
fn estimate_fee_deploy_account() {
    let tx = TxsScenarioBuilder::default().deploy_account().collect();

    let fees = estimate_fees(tx).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.overall_fee, Fee(0));
        assert_eq!(fee.gas_price, GAS_PRICE.price_in_wei);
    }
}

#[test]
fn estimate_fee_combination() {
    let txs = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false, None)
        .declare_class(*ACCOUNT_ADDRESS)
        .declare_deprecated_class(*ACCOUNT_ADDRESS)
        .deploy_account()
        .collect();

    let fees = estimate_fees(txs).expect("Fee estimation should succeed.");
    for fee in fees {
        assert_ne!(fee.overall_fee, Fee(0));
        assert_eq!(fee.gas_price, GAS_PRICE.price_in_wei);
    }
}

#[test]
fn estimate_fee_reverted() {
    let non_existing_contract = contract_address!("0x987");
    let txs = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false, None)
        .invoke_deprecated(*ACCOUNT_ADDRESS, non_existing_contract, None, false, None)
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
        StateNumber::unchecked_right_after_block(BlockNumber(0)),
        BlockNumber(1),
        &get_test_execution_config(),
        false,
        // TODO(yair): Add test for blob fee estimation.
        true,
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
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false, None)
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
        assert_eq!(charge_fee.fee_estimation.gas_price, GAS_PRICE.price_in_wei);

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
            Some(Nonce(stark_felt!(1_u128))),
            false,
            None
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
            Some(Nonce(stark_felt!(1_u128))),
            false,
            None
        )
        // TODO(yair): Find out how to deploy another contract to test calling a new contract.
        .collect();

    let mut result = execute_simulate_transactions(storage_reader, None, txs, None, true, true);
    assert_eq!(result.len(), 2);

    let Some(TransactionSimulationOutput {
        transaction_trace: TransactionTrace::Invoke(invoke_trace),
        fee_estimation: FeeEstimation { overall_fee: invoke_fee_estimation, unit: invoke_unit, .. },
        ..
    }) = result.pop()
    else {
        panic!("Wrong trace type, expected InvokeTransactionTrace.")
    };
    let Some(TransactionSimulationOutput {
        transaction_trace: TransactionTrace::DeployAccount(deploy_account_trace),
        fee_estimation: FeeEstimation { overall_fee: deploy_fee_estimation, unit: deploy_unit, .. },
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

#[test]
fn induced_state_diff() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);
    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[*ACCOUNT_ADDRESS.0.key()]);
    let sequencer_balance_key =
        get_storage_var_address("ERC20_balances", &[*SEQUENCER_ADDRESS.0.key()]);

    // TODO(yair): Add a reverted transaction.
    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false, None)
        .declare_class(*ACCOUNT_ADDRESS)
        .declare_deprecated_class(*ACCOUNT_ADDRESS)
        .deploy_account()
        .collect();
    let simulation_results =
        execute_simulate_transactions(storage_reader, None, tx, None, true, true);
    // This is the value TxsScenarioBuilder uses for the first declared class hash.
    let mut next_declared_class_hash = 100_u128;
    let mut account_balance = u64::try_from(*ACCOUNT_INITIAL_BALANCE).unwrap() as u128;
    let mut sequencer_balance = 0_u128;

    account_balance -= simulation_results[0].fee_estimation.overall_fee.0;
    sequencer_balance += simulation_results[0].fee_estimation.overall_fee.0;
    let expected_invoke_deprecated = ThinStateDiff {
        nonces: indexmap! {*ACCOUNT_ADDRESS => Nonce(stark_felt!(1_u128))},
        deployed_contracts: indexmap! {},
        storage_diffs: indexmap! {
            *TEST_ERC20_CONTRACT_ADDRESS => indexmap!{
                account_balance_key => stark_felt!(account_balance),
                sequencer_balance_key => stark_felt!(sequencer_balance),
            },
        },
        declared_classes: indexmap! {},
        deprecated_declared_classes: vec![],
        replaced_classes: indexmap! {},
    };
    assert_eq!(simulation_results[0].induced_state_diff, expected_invoke_deprecated);

    account_balance -= simulation_results[1].fee_estimation.overall_fee.0;
    sequencer_balance += simulation_results[1].fee_estimation.overall_fee.0;
    let expected_declare_class = ThinStateDiff {
        nonces: indexmap! {*ACCOUNT_ADDRESS => Nonce(stark_felt!(2_u128))},
        declared_classes: indexmap! {class_hash!(next_declared_class_hash) => CompiledClassHash::default()},
        storage_diffs: indexmap! {
            *TEST_ERC20_CONTRACT_ADDRESS => indexmap!{
                account_balance_key => stark_felt!(account_balance),
                sequencer_balance_key => stark_felt!(sequencer_balance),
            },
        },
        deployed_contracts: indexmap! {},
        deprecated_declared_classes: vec![],
        replaced_classes: indexmap! {},
    };
    assert_eq!(simulation_results[1].induced_state_diff, expected_declare_class);
    next_declared_class_hash += 1;

    account_balance -= simulation_results[2].fee_estimation.overall_fee.0;
    sequencer_balance += simulation_results[2].fee_estimation.overall_fee.0;
    let expected_declare_deprecated_class = ThinStateDiff {
        nonces: indexmap! {*ACCOUNT_ADDRESS => Nonce(stark_felt!(3_u128))},
        deprecated_declared_classes: vec![class_hash!(next_declared_class_hash)],
        storage_diffs: indexmap! {
            *TEST_ERC20_CONTRACT_ADDRESS => indexmap!{
                account_balance_key => stark_felt!(account_balance),
                sequencer_balance_key => stark_felt!(sequencer_balance),
            },
        },
        declared_classes: indexmap! {},
        deployed_contracts: indexmap! {},
        replaced_classes: indexmap! {},
    };
    assert_eq!(simulation_results[2].induced_state_diff, expected_declare_deprecated_class);

    let new_account_balance_key =
        get_storage_var_address("ERC20_balances", &[*NEW_ACCOUNT_ADDRESS.0.key()]);
    let new_account_balance = u64::try_from(*ACCOUNT_INITIAL_BALANCE).unwrap() as u128
        - simulation_results[3].fee_estimation.overall_fee.0;

    sequencer_balance += simulation_results[3].fee_estimation.overall_fee.0;
    let expected_deploy_account = ThinStateDiff {
        nonces: indexmap! {*NEW_ACCOUNT_ADDRESS => Nonce(stark_felt!(1_u128))},
        deprecated_declared_classes: vec![],
        storage_diffs: indexmap! {
            *TEST_ERC20_CONTRACT_ADDRESS => indexmap!{
                new_account_balance_key => stark_felt!(new_account_balance),
                sequencer_balance_key => stark_felt!(sequencer_balance),
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
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, true, None)
        .collect();

    let res_only_query =
        execute_simulate_transactions(storage_reader.clone(), None, tx, None, false, false);

    // A tx with only_query=false.
    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(*ACCOUNT_ADDRESS, *DEPRECATED_CONTRACT_ADDRESS, None, false, None)
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
    let storage_address = contract_address!("0x123");
    let class_hash = class_hash!("0x321");
    let blockifier_err = BlockifierTransactionExecutionError::ContractConstructorExecutionFailed {
        error: child,
        storage_address,
        class_hash,
    };
    let err = ExecutionError::from((0, blockifier_err));
    let ExecutionError::TransactionExecutionError { transaction_index, execution_error } = err
    else {
        panic!("unexpected variant")
    };
    assert_eq!(execution_error, expected);
    assert_eq!(transaction_index, 0);

    let child = blockifier::execution::errors::EntryPointExecutionError::RecursionDepthExceeded;
    let selector = EntryPointSelector(stark_felt!("0x111"));
    let blockifier_err = BlockifierTransactionExecutionError::ExecutionError {
        error: child,
        class_hash,
        storage_address,
        selector,
    };
    let expected = format!(
        "Transaction execution has failed:\n{}",
        gen_transaction_execution_error_trace(&blockifier_err)
    );
    let err = ExecutionError::from((0, blockifier_err));
    let ExecutionError::TransactionExecutionError { transaction_index, execution_error } = err
    else {
        panic!("unexpected variant")
    };
    assert_eq!(execution_error, expected);
    assert_eq!(transaction_index, 0);

    let child = blockifier::execution::errors::EntryPointExecutionError::RecursionDepthExceeded;
    let blockifier_err = BlockifierTransactionExecutionError::ValidateTransactionError {
        error: child,
        class_hash,
        storage_address,
        selector,
    };
    let expected = format!(
        "Transaction validation has failed:\n{}",
        gen_transaction_execution_error_trace(&blockifier_err)
    );
    let err = ExecutionError::from((0, blockifier_err));
    let ExecutionError::TransactionExecutionError { transaction_index, execution_error } = err
    else {
        panic!("unexpected variant")
    };
    assert_eq!(execution_error, expected);
    assert_eq!(transaction_index, 0);
}

// Test that we retrieve the correct versioned constants.
#[test]
fn test_get_versioned_constants() {
    let starknet_version_13_0 = StarknetVersion("0.13.0".to_string());
    let starknet_version_13_1 = StarknetVersion("0.13.1".to_string());
    let versioned_constants = get_versioned_constants(Some(&starknet_version_13_0)).unwrap();
    assert_eq!(versioned_constants.invoke_tx_max_n_steps, 3_000_000);
    let versioned_constants = get_versioned_constants(Some(&starknet_version_13_1)).unwrap();
    assert_eq!(versioned_constants.invoke_tx_max_n_steps, 4_000_000);
}
