use assert_matches::assert_matches;
use starknet_api::block::Block;
use starknet_api::core::ChainId;
use test_utils::read_json_file;

use crate::block_hash::{
    calculate_block_hash_by_version,
    calculate_event_commitment_by_version,
    calculate_transaction_commitment_by_version,
    BlockHashError,
    BlockHashVersion,
};

fn validate_block_hash_util(file_name: &str, version: BlockHashVersion) {
    let chain_id = ChainId("SN_MAIN".to_owned());
    let block: Block = serde_json::from_value(read_json_file(file_name)).unwrap();
    let calculated_hash =
        calculate_block_hash_by_version(&block.header, version, &chain_id).unwrap();
    assert_eq!(calculated_hash, block.header.block_hash);

    let calculated_transaction_commitment =
        calculate_transaction_commitment_by_version(&block.body, &version).unwrap();
    assert_eq!(calculated_transaction_commitment, block.header.transaction_commitment.unwrap());

    let calculated_event_commitment =
        calculate_event_commitment_by_version(&block.body.transaction_outputs, &version);
    assert_eq!(calculated_event_commitment, block.header.event_commitment.unwrap());
}

#[test]
fn test_block_hash() {
    validate_block_hash_util("block_hash.json", BlockHashVersion::V3);
}

#[test]
fn test_deprecated_block_hash_v2() {
    validate_block_hash_util("deprecated_block_hash_v2.json", BlockHashVersion::V2);
}

#[test]
fn test_deprecated_block_hash_v1_no_events() {
    validate_block_hash_util("deprecated_block_hash_v1_no_events.json", BlockHashVersion::V1);
}

#[test]
fn test_deprecated_block_hash_v1() {
    validate_block_hash_util("deprecated_block_hash_v1.json", BlockHashVersion::V1);
}

#[test]
fn test_deprecated_block_hash_v0() {
    validate_block_hash_util("deprecated_block_hash_v0.json", BlockHashVersion::V0);
}

#[test]
fn test_missing_header_data() {
    let chain_id = ChainId("SN_MAIN".to_owned());
    let mut block: Block = serde_json::from_value(read_json_file("block_hash.json")).unwrap();
    block.header.transaction_commitment = None;
    let err = calculate_block_hash_by_version(&block.header, BlockHashVersion::V3, &chain_id)
        .unwrap_err();

    assert_matches!(err, BlockHashError::MissingHeaderData);
}
