use assert::assert_ok;

use super::super::test_utils::read_resource::read_resource_file;
use super::block::{Block, BlockStateUpdate};

#[test]
fn load_block_succeeds() {
    assert_ok!(serde_json::from_str::<Block>(&read_resource_file("block.json")));
}

#[test]
fn load_block_state_update_succeeds() {
    assert_ok!(serde_json::from_str::<BlockStateUpdate>(&read_resource_file(
        "block_state_update.json"
    )));
}
