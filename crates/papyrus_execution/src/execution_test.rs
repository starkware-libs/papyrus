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
use starknet_api::block::{BlockBody, BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::patricia_key;
use starknet_api::state::{ContractClass, StateDiff, StateNumber};
use starknet_api::transaction::Calldata;
use test_utils::read_json_file;

use crate::execute_call;

// Based on get_deprecated_contract_class of Blockifier.
// TODO(yair): keep a valid contract class in the repo (in the SN_API format).
fn get_test_deprecated_contract_class() -> DeprecatedContractClass {
    let mut raw_contract_class = read_json_file("deprecated_class.json");
    // ABI is not required for execution.
    raw_contract_class
        .as_object_mut()
        .expect("A compiled contract must be a JSON object.")
        .remove("abi");

    serde_json::from_value(raw_contract_class).unwrap()
}

fn prepare_storage(mut storage_writer: StorageWriter) {
    let class_hash0 = ClassHash(StarkFelt::from(0u128));
    let class0 = serde_json::from_value::<ContractClass>(read_json_file("class.json")).unwrap();
    let casm0 =
        serde_json::from_value::<CasmContractClass>(read_json_file("compiled_class.json")).unwrap();
    let address0 = ContractAddress(patricia_key!("0x0"));

    let class_hash1 = ClassHash(StarkFelt::from(1u128));
    let class1 = get_test_deprecated_contract_class();
    let address1 = ContractAddress(patricia_key!("0x1"));

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(BlockNumber(0), &BlockHeader::default())
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            StateDiff {
                deployed_contracts: indexmap!(address0 => class_hash0, address1 => class_hash1),
                storage_diffs: indexmap!(),
                declared_classes: indexmap!(
                    class_hash0 =>
                    (CompiledClassHash::default(), class0)
                ),
                deprecated_declared_classes: indexmap!(
                    class_hash1 => class1
                ),
                nonces: indexmap!(address0 => Nonce::default(), address1 => Nonce::default()),
                replaced_classes: indexmap!(),
            },
            indexmap!(),
        )
        .unwrap()
        .append_casm(class_hash0, &casm0)
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn execute_call_cairo0() {
    let (storage_reader, storage_writer) = get_test_storage();
    prepare_storage(storage_writer);

    let address1 = ContractAddress(patricia_key!("0x1"));
    let retdata = execute_call(
        storage_reader,
        StateNumber::right_after_block(BlockNumber(0)),
        &address1,
        selector_from_name("without_arg"),
        Calldata::default(),
    )
    .unwrap();

    assert_eq!(retdata, Retdata::default());
}
