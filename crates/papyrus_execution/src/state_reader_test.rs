use blockifier::state::state_api::StateReader;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::{indexmap, IndexMap};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use starknet_api::block::{BlockBody, BlockHash, BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StateNumber};
use starknet_api::{patricia_key, stark_felt};
use test_utils::read_json_file;

use crate::state_reader::ExecutionStateReader;

const CONTRACT_ADDRESS: &str = "0x2";
const DEPRECATED_CONTRACT_ADDRESS: &str = "0x1";

fn get_test_casm() -> CasmContractClass {
    let raw_casm = read_json_file("casm.json");
    serde_json::from_value(raw_casm).unwrap()
}

// A deprecated class for testing, taken from get_deprecated_contract_class of Blockifier.
// todo(yair): Consider saving the json of the result instead of doing this process.
fn get_test_deprecated_contract_class() -> DeprecatedContractClass {
    let mut raw_contract_class = read_json_file("deprecated_class.json");
    // ABI is not required for execution.
    raw_contract_class
        .as_object_mut()
        .expect("A compiled contract must be a JSON object.")
        .remove("abi");

    serde_json::from_value(raw_contract_class).unwrap()
}

#[test]
fn read_state() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();

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
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(0), StateDiff::default(), IndexMap::new())
        .unwrap()
        .append_header(
            BlockNumber(1),
            &BlockHeader {
                block_hash: BlockHash(stark_felt!(1_u128)),
                block_number: BlockNumber(1),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(1), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(1),
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
                nonces: indexmap!(
                    address0 => Nonce::default(),
                    address1 => Nonce::default(),
                ),
                replaced_classes: indexmap!(),
            },
            indexmap!(),
        )
        .unwrap()
        .append_casm(&class_hash0, &casm0)
        .unwrap()
        .append_header(
            BlockNumber(2),
            &BlockHeader {
                block_hash: BlockHash(stark_felt!(2_u128)),
                block_number: BlockNumber(2),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(2), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(2), StateDiff::default(), IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let txn = storage_reader.begin_ro_txn().unwrap();

    let state_number0 = StateNumber::right_after_block(BlockNumber(0));
    let mut state_reader0 = ExecutionStateReader { txn: &txn, state_number: state_number0 };
    // Instead of None the state needs to return a default value.
    let nonce_after_block_0 = state_reader0.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_0, Nonce::default());

    let state_number1 = StateNumber::right_after_block(BlockNumber(1));
    let mut state_reader1 = ExecutionStateReader { txn: &txn, state_number: state_number1 };
    let nonce_after_block_1 = state_reader1.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_1, Nonce::default());

    let state_number1 = StateNumber::right_after_block(BlockNumber(1));
    let mut state_reader1 = ExecutionStateReader { txn: &txn, state_number: state_number1 };
    let nonce_after_block_1 = state_reader1.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_1, Nonce::default());

    let state_number2 = StateNumber::right_after_block(BlockNumber(2));
    let mut state_reader2 = ExecutionStateReader { txn: &txn, state_number: state_number2 };
    let nonce_after_block_2 = state_reader2.get_nonce_at(address0).unwrap();
    assert_eq!(nonce_after_block_2, Nonce::default());
}
