use starknet_api::block::BlockNumber;

use crate::base_layer::{BaseLayerStorageReader, BaseLayerStorageWriter};
use crate::test_utils::get_test_storage;

#[tokio::test]
async fn rw_base_layer_tip_marker() {
    let (reader, mut writer) = get_test_storage().0;

    // Initial tip.
    let initial_tip = reader.begin_ro_txn().unwrap().get_base_layer_block_marker().unwrap();
    assert_eq!(initial_tip, BlockNumber(0));

    // Update tip.
    writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_block_marker(&BlockNumber(5))
        .unwrap()
        .commit()
        .unwrap();
    let updated_tip = reader.begin_ro_txn().unwrap().get_base_layer_block_marker().unwrap();
    assert_eq!(updated_tip, BlockNumber(5));
}

#[test]
fn try_revert_base_layer_marker() {
    let (reader, mut writer) = get_test_storage().0;

    writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_block_marker(&BlockNumber(2))
        .unwrap()
        .try_revert_base_layer_marker(BlockNumber(2))
        .unwrap()
        .commit()
        .unwrap();

    let cur_marker = reader.begin_ro_txn().unwrap().get_base_layer_block_marker().unwrap();
    assert_eq!(cur_marker, BlockNumber(2));

    writer
        .begin_rw_txn()
        .unwrap()
        .try_revert_base_layer_marker(BlockNumber(1))
        .unwrap()
        .commit()
        .unwrap();
    let cur_marker = reader.begin_ro_txn().unwrap().get_base_layer_block_marker().unwrap();
    assert_eq!(cur_marker, BlockNumber(1));
}
