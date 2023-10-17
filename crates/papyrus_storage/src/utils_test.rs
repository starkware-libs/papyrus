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
use crate::utils::{dump_declared_classes_table_by_block_range_internal, DumpDeclaredClass};

#[test]
fn test_dump_table_to_file() {
    let file_path = "tmp_test_dump_declared_classes_table.json";
    let compiled_class_hash = CompiledClassHash(StarkHash::default());
    let mut declared_classes = vec![];
    let mut state_diffs = vec![];
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    for i in 0..5 {
        let i_felt = StarkFelt::from_u128(i as u128);
        declared_classes.push((
            ClassHash(i_felt),
            ContractClass {
                sierra_program: vec![i_felt, i_felt],
                entry_point_by_type: HashMap::new(),
                abi: "".to_string(),
            },
        ));
        state_diffs.push(StateDiff {
            deployed_contracts: indexmap!(),
            storage_diffs: indexmap!(),
            declared_classes: indexmap!(
                declared_classes[i].0 =>
                (compiled_class_hash, declared_classes[i].1.clone()),
            ),
            deprecated_declared_classes: indexmap!(),
            nonces: indexmap!(),
            replaced_classes: indexmap!(),
        });
        let txn = writer.begin_rw_txn().unwrap();
        txn.append_state_diff(BlockNumber(i as u64), state_diffs[i].clone(), indexmap!())
            .unwrap()
            .commit()
            .unwrap();
    }
    let txn = reader.begin_ro_txn().unwrap();
    // Test dump_declared_classes_to_file
    dump_table_to_file(&txn, &txn.tables.declared_classes, file_path).unwrap();
    let file_content = fs::read_to_string(file_path).unwrap();
    let _ = fs::remove_file(file_path);
    assert_eq!(file_content, serde_json::to_string(&declared_classes).unwrap());

    // Test dump_declared_classes_table_by_block_range
    dump_declared_classes_table_by_block_range_internal(&txn, file_path, 2, 4).unwrap();
    let file_content = fs::read_to_string(file_path).unwrap();
    let _ = fs::remove_file(file_path);
    let expected_declared_classes = vec![
        DumpDeclaredClass {
            class_hash: declared_classes[2].0,
            compiled_class_hash,
            sierra_program: declared_classes[2].1.sierra_program.clone(),
            entry_points_by_type: declared_classes[2].1.entry_point_by_type.clone(),
        },
        DumpDeclaredClass {
            class_hash: declared_classes[3].0,
            compiled_class_hash,
            sierra_program: declared_classes[3].1.sierra_program.clone(),
            entry_points_by_type: declared_classes[3].1.entry_point_by_type.clone(),
        },
    ];
    assert_eq!(file_content, serde_json::to_string(&expected_declared_classes).unwrap());
}
