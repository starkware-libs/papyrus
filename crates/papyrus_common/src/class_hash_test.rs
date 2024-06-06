use starknet_api::core::ClassHash;
use starknet_api::state::ContractClass;
use starknet_api::{class_hash, felt};
use test_utils::read_json_file;

use crate::class_hash::calculate_class_hash;

#[test]
fn class_hash() {
    let class: ContractClass = serde_json::from_value(read_json_file("class.json")).unwrap();
    let expected_class_hash =
        class_hash!("0x29927c8af6bccf3f6fda035981e765a7bdbf18a2dc0d630494f8758aa908e2b");
    let calculated_class_hash = calculate_class_hash(&class);
    assert_eq!(calculated_class_hash, expected_class_hash);
}
