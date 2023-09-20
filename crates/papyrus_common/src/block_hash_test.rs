use starknet_api::block::Block;
use starknet_api::core::ChainId;
use test_utils::read_json_file;

use super::calculate_block_hash_by_version;
use crate::block_hash::{calculate_block_commitments, BlockHashVersion};

fn validate_block_hash_util(file_name: &str, version: BlockHashVersion) -> bool {
    let chain_id = ChainId("SN_MAIN".to_owned());
    // Load block with dummy commitments.
    let mut block: Block = serde_json::from_value(read_json_file(file_name)).unwrap();

    let commitments = calculate_block_commitments(&block.header, &block.body).unwrap();
    block.commitments = commitments;

    let calculated_hash =
        calculate_block_hash_by_version(&block.header, &block.commitments, version, &chain_id)
            .unwrap();
    calculated_hash == block.header.block_hash.0
}

#[test]
fn test_block_hash() {
    assert!(validate_block_hash_util("block_hash.json", BlockHashVersion::V3));
}

#[test]
fn test_deprecated_block_hash_v2() {
    assert!(validate_block_hash_util("deprecated_block_hash_v2.json", BlockHashVersion::V2));
}

#[test]
fn test_deprecated_block_hash_v1_no_events() {
    assert!(validate_block_hash_util(
        "deprecated_block_hash_v1_no_events.json",
        BlockHashVersion::V1
    ));
}

#[test]
fn test_deprecated_block_hash_v1() {
    assert!(validate_block_hash_util("deprecated_block_hash_v1.json", BlockHashVersion::V1));
}

#[test]
fn test_deprecated_block_hash_v0() {
    assert!(validate_block_hash_util("deprecated_block_hash_v0.json", BlockHashVersion::V0));
}
