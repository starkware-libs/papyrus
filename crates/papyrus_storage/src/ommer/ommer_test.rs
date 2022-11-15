use starknet_api::{
    BlockNumber, DeclaredContract, Event, StateNumber, Transaction, TransactionOffsetInBlock,
    TransactionOutput,
};

use crate::test_utils::{get_test_block, get_test_state_diff, get_test_storage};
use crate::{
    BodyStorageReader, BodyStorageWriter, OmmerStorageWriter, StateStorageReader,
    StateStorageWriter, StorageReader, StorageResult, ThinStateDiff, ThinTransactionOutput,
    TransactionIndex,
};

// TODO(yair): These functions were written and used in order to experience writing ommer blocks in
// a revert scenario (vs. scenario of raw blocks that need to be written directly to the ommer
// tables). Need to move them to the sync crate and use them in the revert flow (+ moving the
// tests).
type ExtractedBodyData = (Vec<Transaction>, Vec<ThinTransactionOutput>, Vec<Vec<Event>>);
fn extract_body_data_from_storage(
    reader: &StorageReader,
    block_number: BlockNumber,
) -> StorageResult<ExtractedBodyData> {
    let transactions = reader.begin_ro_txn()?.get_block_transactions(block_number)?.unwrap();
    let thin_transaction_outputs =
        reader.begin_ro_txn()?.get_block_transaction_outputs(block_number)?.unwrap();

    // Collect the events into vector of vectors.
    let tx_indices = (0..transactions.len())
        .map(|idx| TransactionIndex(block_number, TransactionOffsetInBlock(idx)));
    let transaction_outputs_events: Vec<Vec<Event>> = tx_indices
        .map(|tx_idx| {
            reader.begin_ro_txn().unwrap().get_transaction_events(tx_idx).unwrap().unwrap()
        })
        .collect();
    Ok((transactions, thin_transaction_outputs, transaction_outputs_events))
}

fn extract_state_diff_data_from_storage(
    reader: &StorageReader,
    block_number: BlockNumber,
) -> StorageResult<(ThinStateDiff, Vec<DeclaredContract>)> {
    let state_number = StateNumber::right_after_block(block_number);

    let thin_state_diff = reader.begin_ro_txn()?.get_state_diff(block_number)?.unwrap();
    let txn = reader.begin_ro_txn()?;
    let state_reader = txn.get_state_reader()?;
    let class_hashes = thin_state_diff.declared_contract_hashes();
    let declared_classes: Vec<DeclaredContract> = class_hashes
        .iter()
        .map(|class_hash| DeclaredContract {
            class_hash: *class_hash,
            contract_class: state_reader
                .get_class_definition_at(state_number, class_hash)
                .unwrap()
                .unwrap(),
        })
        .collect();

    Ok((thin_state_diff, declared_classes))
}

#[test]
fn insert_header_to_ommer() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    let block = get_test_block(7);
    let block_hash = block.header.block_hash;

    writer.begin_rw_txn()?.insert_ommer_header(block_hash, &block.header)?.commit()?;

    Ok(())
}

#[test]
fn move_body_to_ommer() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let block = get_test_block(7);
    let block_number = block.header.block_number;
    let block_hash = block.header.block_hash;

    // Add body to cannonical tables.
    writer.begin_rw_txn()?.append_body(block_number, block.body)?.commit()?;

    let (transactions, thin_transaction_outputs, transaction_outputs_events) =
        extract_body_data_from_storage(&reader, block_number)?;

    writer
        .begin_rw_txn()?
        .insert_ommer_body(
            block_hash,
            &transactions,
            &thin_transaction_outputs,
            &transaction_outputs_events,
        )?
        .commit()?;

    Ok(())
}

#[test]
fn insert_body_to_ommer() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    let block = get_test_block(7);
    let block_hash = block.header.block_hash;
    let body = block.body;
    let transactions = body.transactions().clone();

    fn split_tx_output(tx_output: TransactionOutput) -> (ThinTransactionOutput, Vec<Event>) {
        let events = tx_output.events().to_owned();
        let thin_tx_output = ThinTransactionOutput::from(tx_output);
        (thin_tx_output, events)
    }

    let (thin_tx_outputs, transaction_outputs_events): (Vec<_>, Vec<_>) =
        body.transaction_outputs_into_iter().map(split_tx_output).unzip();

    writer
        .begin_rw_txn()?
        .insert_ommer_body(
            block_hash,
            &transactions,
            &thin_tx_outputs,
            &transaction_outputs_events,
        )?
        .commit()?;

    Ok(())
}

#[test]
fn move_state_diff_to_ommer() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let (header, _, state_diff, declared_classes) = get_test_state_diff();
    let block_number = header.block_number;

    // Add state diff to cannonical tables.
    writer
        .begin_rw_txn()?
        .append_state_diff(block_number, state_diff, declared_classes)?
        .commit()?;

    let (thin_state_diff, declared_classes) =
        extract_state_diff_data_from_storage(&reader, block_number)?;

    // Add the state diff to the ommer tables.
    writer
        .begin_rw_txn()?
        .insert_ommer_state_diff(header.block_hash, &thin_state_diff, &declared_classes)?
        .commit()?;

    Ok(())
}

#[test]
fn insert_raw_state_diff_to_ommer() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    let (header, _, state_diff, declared_classes) = get_test_state_diff();

    let thin_state_diff = ThinStateDiff::from(state_diff);

    // Add the state diff to the ommer tables.
    writer
        .begin_rw_txn()?
        .insert_ommer_state_diff(header.block_hash, &thin_state_diff, &declared_classes)?
        .commit()?;

    Ok(())
}
