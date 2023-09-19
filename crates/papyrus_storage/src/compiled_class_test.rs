use assert_matches::assert_matches;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use pretty_assertions::assert_eq;
use starknet_api::core::ClassHash;
use test_utils::read_json_file;

use crate::compiled_class::{CasmStorageReader, CasmStorageWriter};
use crate::mmap_file::{LocationInFile, Writer};
use crate::test_utils::get_test_storage;
use crate::StorageError;

#[test]
fn append_casm() {
    let casm_json = read_json_file("compiled_class.json");
    let expected_casm: CasmContractClass = serde_json::from_value(casm_json).unwrap();
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let offset = 0;
    let len = writer.file_writers.casm_writer.insert(offset, &expected_casm);
    let location = LocationInFile { offset, len };
    writer.flush_file(crate::OffsetKind::Casm);

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(&ClassHash::default(), &location)
        .unwrap()
        .commit()
        .unwrap();

    let casm = reader.begin_ro_txn().unwrap().get_casm(&ClassHash::default()).unwrap().unwrap();
    assert_eq!(casm, expected_casm);
}

#[test]
fn casm_rewrite() {
    let ((_, mut writer), _temp_dir) = get_test_storage();
    let offset = 0;
    let len = writer.file_writers.casm_writer.insert(offset, &CasmContractClass::default());
    let location = LocationInFile { offset, len };

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(&ClassHash::default(), &location)
        .unwrap()
        .commit()
        .unwrap();

    let Err(err) = writer.begin_rw_txn().unwrap().append_casm(&ClassHash::default(), &location)
    else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(err, StorageError::CompiledClassReWrite{class_hash} if class_hash == ClassHash::default());
}
