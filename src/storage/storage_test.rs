use tempfile::tempdir;

use crate::starknet::BlockNumber;

use super::components::StorageComponents;

#[tokio::test]
async fn test_add_block_number() {
    let dir = tempdir().unwrap();
    let mut storage_components = StorageComponents::new(dir.path());
    let expected = BlockNumber(5);

    storage_components
        .block_storage_writer
        .set_block_number_marker(expected)
        .unwrap();

    let res = storage_components
        .block_storage_reader
        .get_block_number_marker();
    assert_eq!(res.unwrap(), expected);
}
