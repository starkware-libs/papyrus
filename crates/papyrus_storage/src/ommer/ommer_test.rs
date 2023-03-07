use indexmap::IndexMap;
use starknet_api::block::{BlockHeader, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass;
use starknet_api::state::StateNumber;
use starknet_api::transaction::{
    EventContent, Transaction, TransactionOffsetInBlock, TransactionOutput,
};
use test_utils::{get_test_block, get_test_state_diff};

use super::OmmerStorageReader;
use crate::body::events::ThinTransactionOutput;
use crate::body::{BodyStorageReader, BodyStorageWriter};
use crate::ommer::OmmerStorageWriter;
use crate::state::data::ThinStateDiff;
use crate::state::{StateStorageReader, StateStorageWriter};
use crate::test_utils::get_test_storage;
use crate::{StorageReader, TransactionIndex};

// TODO(yair): These functions were written and used in order to experience writing ommer blocks in
// a revert scenario (vs. scenario of raw blocks that need to be written directly to the ommer
// tables). Need to move them to the sync crate and use them in the revert flow (+ moving the
// tests).
fn extract_body_data_from_storage(
    reader: &StorageReader,
    block_number: BlockNumber,
) -> (Vec<Transaction>, Vec<ThinTransactionOutput>, Vec<Vec<EventContent>>) {
    let transactions =
        reader.begin_ro_txn().unwrap().get_block_transactions(block_number).unwrap().unwrap();
    let thin_transaction_outputs = reader
        .begin_ro_txn()
        .unwrap()
        .get_block_transaction_outputs(block_number)
        .unwrap()
        .unwrap();

    // Collect the events into vector of vectors.
    let tx_indices = (0..transactions.len())
        .map(|idx| TransactionIndex(block_number, TransactionOffsetInBlock(idx)));

    let transaction_outputs_events: Vec<Vec<EventContent>> = tx_indices
        .map(|tx_idx| {
            reader
                .begin_ro_txn()
                .unwrap()
                .get_transaction_events(tx_idx)
                .unwrap()
                .unwrap()
                .into_iter()
                .map(|e| e.content)
                .collect()
        })
        .collect();

    (transactions, thin_transaction_outputs, transaction_outputs_events)
}

fn extract_state_diff_data_from_storage(
    reader: &StorageReader,
    block_number: BlockNumber,
) -> (ThinStateDiff, IndexMap<ClassHash, ContractClass>) {
    let state_number = StateNumber::right_after_block(block_number);
    let txn = reader.begin_ro_txn().unwrap();
    let thin_state_diff = txn.get_state_diff(block_number).unwrap().unwrap();
    let state_reader = txn.get_state_reader().unwrap();
    let class_hashes = &thin_state_diff.deprecated_declared_contract_hashes;
    let declared_classes: IndexMap<ClassHash, ContractClass> = class_hashes
        .iter()
        .map(|class_hash| {
            (
                *class_hash,
                state_reader.get_class_definition_at(state_number, class_hash).unwrap().unwrap(),
            )
        })
        .collect();

    (thin_state_diff, declared_classes)
}

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
fn move_body_to_ommer() {
    let (reader, mut writer) = get_test_storage();
    let block = get_test_block(7);
    let block_number = block.header.block_number;
    let block_hash = block.header.block_hash;

    // Add body to cannonical tables.
    writer.begin_rw_txn().unwrap().append_body(block_number, block.body).unwrap().commit().unwrap();

    let (transactions, thin_transaction_outputs, transaction_outputs_events) =
        extract_body_data_from_storage(&reader, block_number);

    writer
        .begin_rw_txn()
        .unwrap()
        .insert_ommer_body(
            block_hash,
            &transactions,
            &thin_transaction_outputs,
            &transaction_outputs_events,
        )
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
fn move_state_diff_to_ommer() {
    let (reader, mut writer) = get_test_storage();
    let header = BlockHeader::default();
    let state_diff = get_test_state_diff();
    let block_number = header.block_number;

    // Add state diff to cannonical tables.
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(block_number, state_diff, IndexMap::new())
        .unwrap()
        .commit()
        .unwrap();

    let (thin_state_diff, declared_classes) =
        extract_state_diff_data_from_storage(&reader, block_number);

    // Add the state diff to the ommer tables.
    writer
        .begin_rw_txn()
        .unwrap()
        .insert_ommer_state_diff(header.block_hash, &thin_state_diff, &declared_classes)
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
