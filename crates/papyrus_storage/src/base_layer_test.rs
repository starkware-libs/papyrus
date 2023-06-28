use starknet_api::block::BlockNumber;

use crate::base_layer::{BaseLayerStorageReader, BaseLayerStorageWriter};
use crate::test_utils::get_test_storage;

#[tokio::test]
async fn rw_base_layer_tip_marker() {
    let (reader, mut writer) = get_test_storage();

    // Initial tip.
    let initial_tip = reader.begin_ro_txn().unwrap().get_base_layer_tip_marker().unwrap();
    assert_eq!(initial_tip, BlockNumber(0));

    // Update tip.
    writer
        .begin_rw_txn()
        .unwrap()
        .update_base_layer_tip_marker(&BlockNumber(5))
        .unwrap()
        .commit()
        .unwrap();
    let updated_tip = reader.begin_ro_txn().unwrap().get_base_layer_tip_marker().unwrap();
    assert_eq!(updated_tip, BlockNumber(5));
}
