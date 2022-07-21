use assert::assert_ok;

use super::super::test_utils::read_resource::read_resource_file;
use super::super::Block;

#[test]
fn load_block() {
    assert_ok!(serde_json::from_str::<Block>(&read_resource_file("block.json")));
}
