use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::test_utils::get_test_storage_by_scope;
use papyrus_storage::StorageScope;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use test_utils::get_test_block;

use crate::{BlockBuilder, BlockBuilderTrait};

#[test]
fn block_proposer() {
    let storage_scope = StorageScope::FullArchive;
    let ((storage_reader, mut storage_writer), _temp_dir) =
        get_test_storage_by_scope(storage_scope);
    let block_body = get_test_block(2, Some(1), None, None).body;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_body(BlockNumber(0), block_body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let proposer = BlockBuilder::new(storage_reader);
    let block_number = BlockNumber(0);
    let proposal_receiver = proposer.build(block_number).unwrap();
    let proposal = proposal_receiver.iter().collect::<Vec<_>>();
    assert_eq!(proposal, block_body.transactions.as_slice());
}

// TODO: add test for non-existing block.
