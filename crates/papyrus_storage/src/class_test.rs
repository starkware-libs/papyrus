use assert_matches::assert_matches;
use indexmap::indexmap;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::StarkHash;
use starknet_api::state::{ContractClass, ThinStateDiff};
use test_utils::read_json_file;

use super::{ClassStorageReader, ClassStorageWriter};
use crate::state::StateStorageWriter;
use crate::test_utils::get_test_storage;
use crate::StorageError;

#[test]
fn append_classes_writes_correct_data() {
    let class_json = read_json_file("class.json");
    let expected_class: ContractClass = serde_json::from_value(class_json).unwrap();
    let deprecated_class_json = read_json_file("deprecated_class.json");
    let expected_deprecated_class: DeprecatedContractClass =
        serde_json::from_value(deprecated_class_json).unwrap();
    let class_hash = ClassHash::default();
    let deprecated_class_hash = ClassHash(StarkHash::ONE);

    let ((reader, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_thin_state_diff(
            BlockNumber(0),
            ThinStateDiff {
                declared_classes: indexmap! { class_hash => CompiledClassHash::default() },
                deprecated_declared_classes: vec![deprecated_class_hash],
                ..Default::default()
            },
        )
        .unwrap()
        .append_classes(
            BlockNumber(0),
            &[(class_hash, &expected_class)],
            &[(deprecated_class_hash, &expected_deprecated_class)],
        )
        .unwrap()
        .commit()
        .unwrap();

    let class = reader.begin_ro_txn().unwrap().get_class(&ClassHash::default()).unwrap().unwrap();
    assert_eq!(class, expected_class);

    let deprecated_class = reader
        .begin_ro_txn()
        .unwrap()
        .get_deprecated_class(&deprecated_class_hash)
        .unwrap()
        .unwrap();
    assert_eq!(deprecated_class, expected_deprecated_class);
}

#[test]
fn append_classes_marker_mismatch() {
    let ((_reader, mut writer), _temp_dir) = get_test_storage();

    let Err(err) = writer
        .begin_rw_txn()
        .unwrap()
        .append_thin_state_diff(BlockNumber(0), ThinStateDiff::default())
        .unwrap()
        .append_classes(BlockNumber(1), &Vec::new(), &Vec::new())
    else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(
        err,
        StorageError::MarkerMismatch { expected, found } if expected.0 == 0 && found.0 == 1
    );
}
