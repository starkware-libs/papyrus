use indexmap::IndexMap;
use starknet_api::block::BlockHeader;
use starknet_api::transaction::{EventContent, TransactionOutput};
use test_utils::{get_test_block, get_test_state_diff};

use super::OmmerStorageReader;
use crate::body::events::ThinTransactionOutput;
use crate::ommer::OmmerStorageWriter;
use crate::state::data::ThinStateDiff;
use crate::test_utils::get_test_storage;

#[test]
fn insert_header_to_ommer() {
    let (_, mut writer) = get_test_storage();
    let block = get_test_block(7);
    let block_hash = block.header.block_hash;

    writer
        .begin_rw_txn()
        .unwrap()
        .insert_ommer_header(block_hash, &block.header)
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn insert_body_to_ommer() {
    let (_, mut writer) = get_test_storage();
    let block = get_test_block(7);

    fn split_tx_output(tx_output: TransactionOutput) -> (ThinTransactionOutput, Vec<EventContent>) {
        let events = tx_output.events().iter().map(|e| e.content.clone()).collect();
        let thin_tx_output = ThinTransactionOutput::from(tx_output);
        (thin_tx_output, events)
    }

    let (thin_tx_outputs, transaction_outputs_events): (Vec<_>, Vec<_>) =
        block.body.transaction_outputs.into_iter().map(split_tx_output).unzip();

    writer
        .begin_rw_txn()
        .unwrap()
        .insert_ommer_body(
            block.header.block_hash,
            &block.body.transactions,
            &thin_tx_outputs,
            &transaction_outputs_events,
        )
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn insert_raw_state_diff_to_ommer() {
    let (_, mut writer) = get_test_storage();
    let header = BlockHeader::default();
    let state_diff = get_test_state_diff();

    let thin_state_diff = ThinStateDiff::from(state_diff);

    // Add the state diff to the ommer tables.
    writer
        .begin_rw_txn()
        .unwrap()
        .insert_ommer_state_diff(header.block_hash, &thin_state_diff, &IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();
}

#[test]
fn get_ommer_header() {
    let (reader, mut writer) = get_test_storage();
    let block = get_test_block(7);
    let block_hash = block.header.block_hash;

    assert!(reader.begin_ro_txn().unwrap().get_ommer_header(block_hash).unwrap().is_none());

    writer
        .begin_rw_txn()
        .unwrap()
        .insert_ommer_header(block_hash, &block.header)
        .unwrap()
        .commit()
        .unwrap();

    assert_eq!(
        reader.begin_ro_txn().unwrap().get_ommer_header(block_hash).unwrap().unwrap(),
        block.header
    );
}
