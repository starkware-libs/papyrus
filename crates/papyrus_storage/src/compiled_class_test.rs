use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use pretty_assertions::assert_eq;
use starknet_api::core::ClassHash;
use test_utils::read_json_file;

use crate::compiled_class::{CasmStorageReader, CasmStorageWriter};
use crate::db::{DbError, KeyAlreadyExistsError};
use crate::test_utils::get_test_storage;
use crate::StorageError;

#[test]
fn append_casm() {
    let casm_json = read_json_file("compiled_class.json");
    let expected_casm: CasmContractClass = serde_json::from_value(casm_json).unwrap();
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(&ClassHash::default(), &expected_casm)
        .unwrap()
        .commit()
        .unwrap();

    let casm = reader.begin_ro_txn().unwrap().get_casm(&ClassHash::default()).unwrap().unwrap();
    assert_eq!(casm, expected_casm);
}

#[test]
fn casm_rewrite() {
    let ((_, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(&ClassHash::default(), &CasmContractClass::default())
        .unwrap()
        .commit()
        .unwrap();

    let Err(err) = writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(&ClassHash::default(), &CasmContractClass::default())
    else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(err, StorageError::InnerError(DbError::KeyAlreadyExists(KeyAlreadyExistsError {
        table_name: _,
        key,
        value: _
    })) if key == format!("{:?}", ClassHash::default()));
}
