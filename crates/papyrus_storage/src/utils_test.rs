use std::collections::HashMap;
use std::fs;

use indexmap::indexmap;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff};

use super::dump_table_to_file;
use crate::state::StateStorageWriter;
use crate::test_utils::get_test_storage;

#[test]
fn test_dump_table_to_file() {
    let file_path = "tmp_test_dump_declared_classes_table.json";
    let declared_class1 = (
        ClassHash(1u128.into()),
        ContractClass {
            sierra_program: vec![StarkFelt::ONE, StarkFelt::TWO],
            entry_point_by_type: HashMap::new(),
            abi: "".to_string(),
        },
    );
    let declared_class2 = (
        ClassHash(2u128.into()),
        ContractClass {
            sierra_program: vec![StarkFelt::THREE, StarkFelt::ZERO],
            entry_point_by_type: HashMap::new(),
            abi: "".to_string(),
        },
    );
    let compiled_class_hash = CompiledClassHash(StarkHash::default());
    let declared_classes = vec![declared_class1.clone(), declared_class2.clone()];
    let declared_classes_for_append_state = indexmap!(
        declared_class1.0 =>
        (compiled_class_hash, declared_class1.1.clone()),
        declared_class2.0 =>
        (compiled_class_hash, declared_class2.1.clone()),
    );

    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let txn = writer.begin_rw_txn().unwrap();
    txn.append_state_diff(
        BlockNumber(0),
        StateDiff {
            deployed_contracts: indexmap!(),
            storage_diffs: indexmap!(),
            declared_classes: declared_classes_for_append_state,
            deprecated_declared_classes: indexmap!(),
            nonces: indexmap!(),
            replaced_classes: indexmap!(),
        },
        indexmap!(),
    )
    .unwrap()
    .commit()
    .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    dump_table_to_file(&txn, &txn.tables.declared_classes, file_path).unwrap();
    let file_content = fs::read_to_string(file_path).unwrap();
    let _ = fs::remove_file(file_path);
    assert_eq!(file_content, serde_json::to_string(&declared_classes).unwrap());
}
