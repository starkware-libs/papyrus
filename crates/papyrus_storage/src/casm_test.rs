use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;
use test_utils::read_json_file;

use crate::casm::{CasmStorageReader, CasmStorageWriter};
use crate::test_utils::get_test_storage;
use crate::StorageError;

#[test]
fn append_casm() {
    let casm_json = read_json_file("compiled_class.json");
    let expected_casm: CasmContractClass = serde_json::from_value(casm_json).unwrap();
    let (reader, mut writer) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(ClassHash::default(), &expected_casm)
        .unwrap()
        .commit()
        .unwrap();

    let casm = reader.begin_ro_txn().unwrap().get_casm(ClassHash::default()).unwrap().unwrap();
    assert_eq!(casm, expected_casm);
}

#[test]
fn casm_rewrite() {
    let (_, mut writer) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(ClassHash::default(), &CasmContractClass::default())
        .unwrap()
        .commit()
        .unwrap();

    let err = writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(ClassHash::default(), &CasmContractClass::default())
        // A workaround because StorageTxn doesn't implement Debug.
        .map(|_| ())
        .unwrap_err();

    assert_matches!(err, StorageError::CompiledClassReWrite{class_hash} if class_hash == ClassHash::default());
}
