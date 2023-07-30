use std::sync::Arc;

use blockifier::abi::abi_utils::selector_from_name;
use blockifier::execution::entry_point::Retdata;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use starknet_api::block::{BlockBody, BlockHeader, BlockNumber, BlockTimestamp, GasPrice};
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StateNumber};
use starknet_api::transaction::Calldata;
use starknet_api::{calldata, patricia_key, stark_felt};
use test_utils::read_json_file;

use crate::execute_call;

const CHAIN_ID: &str = "TEST_CHAIN_ID";
const GAS_PRICE: GasPrice = GasPrice(100 * u128::pow(10, 9)); // Given in units of wei.
const BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234);
const SEQUENCER_ADDRESS: &str = "0xa";
const DEPRECATED_CONTRACT_ADDRESS: &str = "0x1";
const CONTRACT_ADDRESS: &str = "0x2";

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
                    address0 => class_hash0,
                    address1 => class_hash1,
                ),
                storage_diffs: indexmap!(),
                declared_classes: indexmap!(
                    class_hash0 =>
                    (compiled_class_hash0, class0)
                ),
                deprecated_declared_classes: indexmap!(
                    class_hash1 => class1,
                ),
                nonces: indexmap!(),
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
