use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;
use test_utils::read_json_file;

use crate::compiled_class::{CompiledClassStorageReader, CompiledClassStorageWriter};
use crate::test_utils::get_test_storage;

#[test]
fn append_compiled_class() {
    let casm_json = read_json_file("casm_contract_class.json");
    let expected_casm: CasmContractClass = serde_json::from_value(casm_json).unwrap();
    let (reader, mut writer) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_compiled_class(ClassHash::default(), &expected_casm)
        .unwrap()
        .commit()
        .unwrap();

    let casm =
        reader.begin_ro_txn().unwrap().get_compiled_class(ClassHash::default()).unwrap().unwrap();
    assert_eq!(casm, expected_casm);
}
