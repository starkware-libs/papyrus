use indexmap::indexmap;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::hash::StarkHash;
use starknet_api::state::{ContractClass, StateDiff};
use tempfile::TempDir;

use crate::{create_data_files, get_data_file_path, ThinStateDiffIterator};

#[test]
fn test_state_diff_iterator() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    let iter = ThinStateDiffIterator::new(BlockNumber(0), BlockNumber(5), &reader);
    assert!(get_state_diffs_in_iter(iter).is_empty());

    append_state_diff_with_declared_class_identifier(&mut writer, 0);
    append_state_diff_with_declared_class_identifier(&mut writer, 1);

    let iter = ThinStateDiffIterator::new(BlockNumber(0), BlockNumber(5), &reader);
    assert_eq!(get_state_diffs_in_iter(iter), vec![0, 1]);

    let iter = ThinStateDiffIterator::new(BlockNumber(0), BlockNumber(1), &reader);
    assert_eq!(get_state_diffs_in_iter(iter), vec![0]);

    let iter = ThinStateDiffIterator::new(BlockNumber(1), BlockNumber(5), &reader);
    assert_eq!(get_state_diffs_in_iter(iter), vec![1]);

    let iter = ThinStateDiffIterator::new(BlockNumber(2), BlockNumber(5), &reader);
    assert!(get_state_diffs_in_iter(iter).is_empty());
}

fn append_state_diff_with_declared_class_identifier(
    storage_writer: &mut StorageWriter,
    identifier: usize,
) {
    let state_diff = StateDiff {
        declared_classes: indexmap!(
            ClassHash(StarkHash::from(identifier as u128)) =>
            <(CompiledClassHash, ContractClass)>::default(),
        ),
        ..Default::default()
    };

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(identifier as u64), state_diff, indexmap!())
        .unwrap()
        .commit()
        .unwrap();
}

fn get_state_diffs_in_iter(iter: ThinStateDiffIterator<'_>) -> Vec<usize> {
    let mut identifiers = vec![];
    for state_diff in iter {
        identifiers.push(usize::from_be_bytes(
            state_diff.declared_classes.first().unwrap().0.0.bytes()[24..32].try_into().unwrap(),
        ));
    }
    identifiers
}

#[test]
fn create_files_test() {
    const BYTES_NUM: usize = 30;
    const FILE_SIZE_LIMIT: usize = 10;

    let temp_dir = TempDir::new().unwrap();
    let bytes_iter = BytesIterator { left: BYTES_NUM };
    let returned_files = create_data_files(&temp_dir, bytes_iter, 10);

    let expected_file_number = BYTES_NUM / FILE_SIZE_LIMIT;
    assert_eq!(returned_files.len(), expected_file_number);
    for (idx, returned_file_path) in returned_files.iter().enumerate().take(expected_file_number) {
        let file_name = get_data_file_path(&temp_dir, idx);
        assert_eq!(returned_file_path, &file_name);
        assert_eq!(std::fs::metadata(file_name).unwrap().len(), FILE_SIZE_LIMIT as u64);
    }
    struct BytesIterator {
        left: usize,
    }

    impl Iterator for BytesIterator {
        type Item = u8;

        fn next(&mut self) -> Option<Self::Item> {
            if self.left == 0 {
                return None;
            }
            self.left -= 1;
            Some(0)
        }
    }
}
