use std::collections::HashMap;
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::abi::abi_utils::get_storage_var_address;
use blockifier::execution::entry_point::Retdata;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::{StorageReader, StorageWriter};
use starknet_api::block::{BlockBody, BlockHeader, BlockNumber, BlockTimestamp, GasPrice};
use starknet_api::core::{
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StateNumber};
use starknet_api::transaction::{
    Calldata,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeployAccountTransaction,
    Fee,
    InvokeTransaction,
    InvokeTransactionV1,
    TransactionVersion,
};
use starknet_api::{calldata, class_hash, contract_address, patricia_key, stark_felt};
use test_utils::read_json_file;

use crate::execution_utils::selector_from_name;
use crate::objects::{
    DeclareTransactionTrace, DeployAccountTransactionTrace, InvokeTransactionTrace,
    TransactionTrace,
};
use crate::{estimate_fee, execute_call, simulate_transactions, ExecutableTransactionInput};

const CHAIN_ID: &str = "TEST_CHAIN_ID";
const GAS_PRICE: GasPrice = GasPrice(100 * u128::pow(10, 9)); // Given in units of wei.
const MAX_FEE: u128 = 1000000 * GAS_PRICE.0;
const BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234);
const SEQUENCER_ADDRESS: &str = "0xa";
const DEPRECATED_CONTRACT_ADDRESS: &str = "0x1";
const CONTRACT_ADDRESS: &str = "0x2";
const ACCOUNT_CLASS_HASH: &str = "0x333";
const ACCOUNT_ADDRESS: &str = "0x444";
const NEW_ACCOUNT_ADDRESS: &str =
    "0x0153ade9ef510502c4f3b879c049dcc3ad5866706cae665f0d9df9b01e794fdb";

// A deprecated class for testing, taken from get_deprecated_contract_class of Blockifier.
// TODO(yair): Save the json after the abi removal.
fn get_test_deprecated_contract_class() -> DeprecatedContractClass {
    let mut raw_contract_class = read_json_file("deprecated_class.json");
    // ABI is not required for execution.
    raw_contract_class
        .as_object_mut()
        .expect("A compiled contract must be a JSON object.")
        .remove("abi");

    serde_json::from_value(raw_contract_class).unwrap()
}
fn get_test_casm() -> CasmContractClass {
    let raw_casm = read_json_file("casm.json");
    serde_json::from_value(raw_casm).unwrap()
}
fn get_test_erc20_fee_contract_class() -> DeprecatedContractClass {
    let mut raw_contract_class = read_json_file("erc20_fee_contract_class.json");
    // ABI is not required for execution.
    raw_contract_class
        .as_object_mut()
        .expect("A compiled contract must be a JSON object.")
        .remove("abi");

    serde_json::from_value(raw_contract_class).unwrap()
}
// An account class for testing.
// TODO(yair): Save the json after the abi removal.
fn get_test_account_class() -> DeprecatedContractClass {
    let mut raw_contract_class = read_json_file("account_class.json");
    // ABI is not required for execution.
    raw_contract_class
        .as_object_mut()
        .expect("A compiled contract must be a JSON object.")
        .remove("abi");

    serde_json::from_value(raw_contract_class).unwrap()
}

const TEST_ERC20_CONTRACT_CLASS_HASH: &str = "0x1010";
const TEST_ERC20_CONTRACT_ADDRESS: &str = "0x1001";
const ACCOUNT_INITIAL_BALANCE: u128 = 2 * MAX_FEE;

fn prepare_storage(mut storage_writer: StorageWriter) {
    let class_hash0 = ClassHash(2u128.into());
    let address0 = ContractAddress(patricia_key!(CONTRACT_ADDRESS));
    // The class is not used in the execution, so it can be default.
    let class0 = ContractClass::default();
    let casm0 = get_test_casm();
    let compiled_class_hash0 = CompiledClassHash(StarkHash::default());

    let class_hash1 = ClassHash(1u128.into());
    let class1 = get_test_deprecated_contract_class();
    let address1 = ContractAddress(patricia_key!(DEPRECATED_CONTRACT_ADDRESS));

    let account_class_hash = ClassHash(StarkHash::try_from(ACCOUNT_CLASS_HASH).unwrap());
    let account_class = get_test_account_class();
    let account_address = ContractAddress(patricia_key!(ACCOUNT_ADDRESS));

    let erc20_fee_contract_class_hash = class_hash!(TEST_ERC20_CONTRACT_CLASS_HASH);
    let erc20_fee_contract_class = get_test_erc20_fee_contract_class();
    let erc20_fee_contract_address = contract_address!(TEST_ERC20_CONTRACT_ADDRESS);
    let minter_var_address = get_storage_var_address("permitted_minter", &[])
        .expect("Failed to get permitted_minter storage address.");

    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[stark_felt!(ACCOUNT_ADDRESS)]).unwrap();
    let new_account_balance_key =
        get_storage_var_address("ERC20_balances", &[stark_felt!(NEW_ACCOUNT_ADDRESS)]).unwrap();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                gas_price: GAS_PRICE,
                sequencer: ContractAddress(patricia_key!(SEQUENCER_ADDRESS)),
                timestamp: BLOCK_TIMESTAMP,
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
                    erc20_fee_contract_address => erc20_fee_contract_class_hash,
                    address0 => class_hash0,
                    address1 => class_hash1,
                    account_address => account_class_hash,
                ),
                storage_diffs: indexmap!(
                    erc20_fee_contract_address => indexmap!(
                        // Give the accounts some balance.
                        account_balance_key => stark_felt!(ACCOUNT_INITIAL_BALANCE),
                        new_account_balance_key => stark_felt!(ACCOUNT_INITIAL_BALANCE),
                        // Give the first account mint permission (what is this?).
                        minter_var_address => *account_address.0.key()
                    ),
                ),
                declared_classes: indexmap!(
                    class_hash0 =>
                    (compiled_class_hash0, class0)
                ),
                deprecated_declared_classes: indexmap!(
                    erc20_fee_contract_class_hash => erc20_fee_contract_class,
                    class_hash1 => class1,
                    account_class_hash => account_class,
                ),
                nonces: indexmap!(
                    erc20_fee_contract_address => Nonce::default(),
                    address0 => Nonce::default(),
                    address1 => Nonce::default(),
                    account_address => Nonce::default(),
                ),
                replaced_classes: indexmap!(),
            },
            indexmap!(),
        )
        .unwrap()
        .append_casm(&class_hash0, &casm0)
        .unwrap()
        .commit()
        .unwrap();
}

// Test calling entry points of a deprecated class.
#[test]
fn execute_call_cairo0() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let chain_id = ChainId(CHAIN_ID.to_string());

    // Test that the entry point can be called without arguments.
    let address1 = ContractAddress(patricia_key!(DEPRECATED_CONTRACT_ADDRESS));
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &address1,
        selector_from_name("without_arg"),
        Calldata::default(),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata::default());

    // Test that the entry point can be called with arguments.
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &address1,
        selector_from_name("with_arg"),
        Calldata(Arc::new(vec![StarkFelt::from(25u128)])),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata::default());

    // Test that the entry point can return a result.
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &address1,
        selector_from_name("return_result"),
        Calldata(Arc::new(vec![StarkFelt::from(123u128)])),
    )
    .unwrap()
    .retdata;
    assert_eq!(retdata, Retdata(vec![StarkFelt::from(123u128)]));

    // Test that the entry point can read and write to the contract storage.
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &address1,
        selector_from_name("test_storage_read_write"),
        Calldata(Arc::new(vec![StarkFelt::from(123u128), StarkFelt::from(456u128)])),
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

    let chain_id = ChainId(CHAIN_ID.to_string());

    // Test that the entry point can read and write to the contract storage.
    let address0 = ContractAddress(patricia_key!(CONTRACT_ADDRESS));
    let retdata = execute_call(
        &storage_reader.begin_ro_txn().unwrap(),
        &chain_id,
        StateNumber::right_after_block(BlockNumber(0)),
        &address0,
        selector_from_name("test_storage_read_write"),
        calldata,
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
        .invoke_deprecated(
            ContractAddress(patricia_key!(ACCOUNT_ADDRESS)),
            ContractAddress(patricia_key!(DEPRECATED_CONTRACT_ADDRESS)),
            None,
        )
        .collect();
    let fees = estimate_fees(tx);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, GAS_PRICE);
    }
}

#[test]
fn estimate_fee_declare_deprecated_class() {
    let tx = TxsScenarioBuilder::default()
        .declare_deprecated_class(ContractAddress(patricia_key!(ACCOUNT_ADDRESS)))
        .collect();

    let fees = estimate_fees(tx);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, GAS_PRICE);
    }
}

#[test]
fn estimate_fee_declare_class() {
    let tx = TxsScenarioBuilder::default()
        .declare_class(ContractAddress(patricia_key!(ACCOUNT_ADDRESS)))
        .collect();

    let fees = estimate_fees(tx);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, GAS_PRICE);
    }
}

#[test]
fn estimate_fee_deploy_account() {
    let tx = TxsScenarioBuilder::default().deploy_account().collect();

    let fees = estimate_fees(tx);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, GAS_PRICE);
    }
}

#[test]
fn estimate_fee_combination() {
    let sender_address = ContractAddress(patricia_key!(ACCOUNT_ADDRESS));
    let txs = TxsScenarioBuilder::default()
        .invoke_deprecated(
            sender_address,
            ContractAddress(patricia_key!(DEPRECATED_CONTRACT_ADDRESS)),
            None,
        )
        .declare_class(sender_address)
        .declare_deprecated_class(sender_address)
        .deploy_account()
        .collect();

    let fees = estimate_fees(txs);
    for fee in fees {
        assert_ne!(fee.1, Fee(0));
        assert_eq!(fee.0, GAS_PRICE);
    }
}

fn estimate_fees(txs: Vec<ExecutableTransactionInput>) -> Vec<(GasPrice, Fee)> {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let chain_id = ChainId(CHAIN_ID.to_string());
    let storage_txn = storage_reader.begin_ro_txn().unwrap();

    estimate_fee(txs, &chain_id, &storage_txn, StateNumber::right_after_block(BlockNumber(0)))
        .unwrap()
}

// Creates transactions for testing while resolving nonces and class hashes uniqueness.
struct TxsScenarioBuilder {
    // Each transaction by the same sender needs a unique nonce.
    sender_to_nonce: HashMap<ContractAddress, u128>,
    // Each declare class needs a unique class hash.
    next_class_hash: u128,
    // the result.
    txs: Vec<ExecutableTransactionInput>,
}

impl Default for TxsScenarioBuilder {
    fn default() -> Self {
        Self { sender_to_nonce: HashMap::new(), next_class_hash: 100_u128, txs: Vec::new() }
    }
}

impl TxsScenarioBuilder {
    pub fn collect(&self) -> Vec<ExecutableTransactionInput> {
        self.txs.clone()
    }

    pub fn invoke_deprecated(
        mut self,
        sender_address: ContractAddress,
        contract_address: ContractAddress,
        nonce: Option<Nonce>,
    ) -> Self {
        let calldata = calldata![
            *contract_address.0.key(),             // Contract address.
            selector_from_name("return_result").0, // EP selector.
            stark_felt!(1_u8),                     // Calldata length.
            stark_felt!(2_u8)                      // Calldata: num.
        ];
        let nonce = match nonce {
            None => self.next_nonce(sender_address),
            Some(nonce) => {
                let override_next_nonce: u128 =
                    u64::try_from(nonce.0).expect("Nonce should fit in u64.").into();
                self.sender_to_nonce.insert(sender_address, override_next_nonce + 1);
                nonce
            }
        };
        let tx = ExecutableTransactionInput::Invoke(InvokeTransaction::V1(InvokeTransactionV1 {
            calldata,
            max_fee: Fee(MAX_FEE),
            sender_address,
            nonce,
            ..Default::default()
        }));
        self.txs.push(tx);
        self
    }

    pub fn declare_deprecated_class(mut self, sender_address: ContractAddress) -> Self {
        let tx = ExecutableTransactionInput::DeclareV1(
            DeclareTransactionV0V1 {
                max_fee: Fee(MAX_FEE),
                sender_address,
                nonce: self.next_nonce(sender_address),
                class_hash: self.next_class_hash(),
                ..Default::default()
            },
            get_test_deprecated_contract_class(),
        );
        self.txs.push(tx);
        self
    }

    pub fn declare_class(mut self, sender_address: ContractAddress) -> TxsScenarioBuilder {
        let tx = ExecutableTransactionInput::DeclareV2(
            DeclareTransactionV2 {
                max_fee: Fee(MAX_FEE),
                sender_address,
                nonce: self.next_nonce(sender_address),
                class_hash: self.next_class_hash(),
                ..Default::default()
            },
            get_test_casm(),
        );
        self.txs.push(tx);
        self
    }

    pub fn deploy_account(mut self) -> TxsScenarioBuilder {
        let tx = ExecutableTransactionInput::Deploy(DeployAccountTransaction {
            max_fee: Fee(MAX_FEE),
            nonce: Nonce(stark_felt!(0_u128)),
            class_hash: ClassHash(StarkHash::try_from(ACCOUNT_CLASS_HASH).unwrap()),
            version: TransactionVersion(1_u128.into()),
            ..Default::default()
        });
        self.txs.push(tx);
        self
    }

    fn next_nonce(&mut self, sender_address: ContractAddress) -> Nonce {
        match self.sender_to_nonce.get_mut(&sender_address) {
            Some(current) => {
                let res = Nonce(stark_felt!(*current));
                *current += 1;
                res
            }
            None => {
                self.sender_to_nonce.insert(sender_address, 1);
                Nonce(stark_felt!(0_u128))
            }
        }
    }

    fn next_class_hash(&mut self) -> ClassHash {
        let class_hash = ClassHash(self.next_class_hash.into());
        self.next_class_hash += 1;
        class_hash
    }
}

fn execute_simulate_transactions(
    storage_reader: &StorageReader,
    txs: Vec<ExecutableTransactionInput>,
    charge_fee: bool,
    validate: bool,
) -> Vec<(TransactionTrace, GasPrice, Fee)> {
    let chain_id = ChainId(CHAIN_ID.to_string());
    let storage_txn = storage_reader.begin_ro_txn().unwrap();

    simulate_transactions(
        txs,
        &chain_id,
        &storage_txn,
        StateNumber::right_after_block(BlockNumber(0)),
        Some(contract_address!(TEST_ERC20_CONTRACT_ADDRESS)),
        charge_fee,
        validate,
    )
    .unwrap()
}

#[test]
fn simulate_invoke() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default()
        .invoke_deprecated(
            ContractAddress(patricia_key!(ACCOUNT_ADDRESS)),
            ContractAddress(patricia_key!(DEPRECATED_CONTRACT_ADDRESS)),
            None,
        )
        .collect();
    let exec_only_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), false, false);
    let validate_results = execute_simulate_transactions(&storage_reader, tx.clone(), false, true);
    let charge_fee_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(&storage_reader, tx, true, true);

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
                execute_invocation: Ok(_),
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
                execute_invocation: Ok(_),
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
                execute_invocation: Ok(_),
                fee_transfer_invocation: Some(_),
            }
        );
        assert_eq!(charge_fee.1, GAS_PRICE);

        assert_eq!(
            exec_only_trace.execute_invocation.as_ref().unwrap(),
            charge_fee_trace.execute_invocation.as_ref().unwrap()
        );

        let TransactionTrace::Invoke(charge_fee_validate_trace) = &charge_fee_validate.0 else {
            panic!("Wrong trace type, expected InvokeTransactionTrace.")
        };
        assert_matches!(
            charge_fee_validate_trace,
            InvokeTransactionTrace {
                validate_invocation: Some(_),
                execute_invocation: Ok(_),
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

    let tx = TxsScenarioBuilder::default()
        .declare_deprecated_class(ContractAddress(patricia_key!(ACCOUNT_ADDRESS)))
        .collect();
    let exec_only_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), false, false);
    let validate_results = execute_simulate_transactions(&storage_reader, tx.clone(), false, true);
    let charge_fee_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(&storage_reader, tx, true, true);

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

#[test]
fn simulate_deploy_account() {
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    prepare_storage(storage_writer);

    let tx = TxsScenarioBuilder::default().deploy_account().collect();
    let exec_only_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), false, false);
    let validate_results = execute_simulate_transactions(&storage_reader, tx.clone(), false, true);
    let charge_fee_results =
        execute_simulate_transactions(&storage_reader, tx.clone(), true, false);
    let charge_fee_validate_results =
        execute_simulate_transactions(&storage_reader, tx, true, true);

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

#[test]
fn simulate_invoke_from_new_account() {
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
            ContractAddress(patricia_key!(DEPRECATED_CONTRACT_ADDRESS)),
            // the deploy account make the next nonce be 1.
            Some(Nonce(stark_felt!(1_u128)))
        )
        // TODO(yair): Find out how to deploy another contract to test calling a new contract.
        .collect();

    let mut result = execute_simulate_transactions(&storage_reader, txs, false, false);
    assert_eq!(result.len(), 2);

    let Some((TransactionTrace::Invoke(invoke_trace), _, _)) = result.pop() else {
        panic!("Wrong trace type, expected InvokeTransactionTrace.")
    };
    let Some((TransactionTrace::DeployAccount(deploy_account_trace), _, _)) = result.pop() else {
        panic!("Wrong trace type, expected DeployAccountTransactionTrace.")
    };

    assert_eq!(
        deploy_account_trace.constructor_invocation.function_call.contract_address,
        new_account_address
    );

    // Check that the invoke transaction succeeded.
    invoke_trace.execute_invocation.unwrap();
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
            ContractAddress(patricia_key!(DEPRECATED_CONTRACT_ADDRESS)),
            // the deploy account make the next nonce be 1.
            Some(Nonce(stark_felt!(1_u128)))
        )
        // TODO(yair): Find out how to deploy another contract to test calling a new contract.
        .collect();

    let mut result = execute_simulate_transactions(&storage_reader, txs, true, true);
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
    invoke_trace.execute_invocation.unwrap();

    // Check that the fee was charged.
    assert_ne!(deploy_fee_estimation, Fee(0));
    assert_matches!(deploy_account_trace.fee_transfer_invocation, Some(_));
    assert_ne!(invoke_fee_estimation, Fee(0));
    assert_matches!(invoke_trace.fee_transfer_invocation, Some(_));
}
