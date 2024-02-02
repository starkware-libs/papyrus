use std::collections::HashMap;
use std::fs;

use indexmap::indexmap;
use metrics_exporter_prometheus::PrometheusBuilder;
use pretty_assertions::assert_eq;
use prometheus_parse::Value::{Counter, Gauge};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::state::{ContractClass, StateDiff};
use starknet_types_core::felt::Felt;
use test_utils::prometheus_is_contained;

use super::update_storage_metrics;
use crate::state::StateStorageWriter;
use crate::test_utils::get_test_storage;
use crate::utils::{dump_declared_classes_table_by_block_range_internal, DumpDeclaredClass};

// TODO(yael): fix dump_table_to_file.
#[test]
fn test_dump_declared_classes() {
    let file_path = "tmp_test_dump_declared_classes_table.json";
    let compiled_class_hash = CompiledClassHash(Felt::default());
    let mut declared_classes = vec![];
    let mut state_diffs = vec![];
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    for i in 0..5 {
        let i_felt = Felt::from(i as u128);
        declared_classes.push((
            ClassHash(i_felt),
            ContractClass {
                sierra_program: vec![i_felt, i_felt],
                entry_points_by_type: HashMap::new(),
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

    // Test dump_declared_classes_table_by_block_range
    dump_declared_classes_table_by_block_range_internal(&txn, file_path, 2, 4).unwrap();
    let file_content = fs::read_to_string(file_path).unwrap();
    let _ = fs::remove_file(file_path);
    let expected_declared_classes = vec![
        DumpDeclaredClass {
            class_hash: declared_classes[2].0,
            compiled_class_hash,
            sierra_program: declared_classes[2].1.sierra_program.clone(),
            entry_points_by_type: declared_classes[2].1.entry_points_by_type.clone(),
        },
        DumpDeclaredClass {
            class_hash: declared_classes[3].0,
            compiled_class_hash,
            sierra_program: declared_classes[3].1.sierra_program.clone(),
            entry_points_by_type: declared_classes[3].1.entry_points_by_type.clone(),
        },
    ];
    assert_eq!(file_content, serde_json::to_string(&expected_declared_classes).unwrap());
}

#[test]
fn update_storage_metrics_test() {
    let ((reader, _writer), _temp_dir) = get_test_storage();
    let handle = PrometheusBuilder::new().install_recorder().unwrap();

    assert!(prometheus_is_contained(handle.render(), "storage_free_pages_number", &[]).is_none());
    assert!(prometheus_is_contained(handle.render(), "storage_last_page_number", &[]).is_none());
    assert!(
        prometheus_is_contained(handle.render(), "storage_last_transaction_index", &[]).is_none()
    );

    update_storage_metrics(&reader).unwrap();

    let Gauge(free_pages) =
        prometheus_is_contained(handle.render(), "storage_free_pages_number", &[]).unwrap()
    else {
        panic!("storage_free_pages_number is not a Gauge")
    };
    // TODO(dvir): add an upper limit when the bug in the binding freelist function will be fixed.
    assert!(0f64 < free_pages);

    let Counter(last_page) =
        prometheus_is_contained(handle.render(), "storage_last_page_number", &[]).unwrap()
    else {
        panic!("storage_last_page_number is not a Counter")
    };
    assert!(0f64 < last_page);
    assert!(last_page < 1000f64);

    let Counter(last_transaction) =
        prometheus_is_contained(handle.render(), "storage_last_transaction_index", &[]).unwrap()
    else {
        panic!("storage_last_transaction_index is not a Counter")
    };
    assert!(0f64 < last_transaction);
    assert!(last_transaction < 100f64);
}
