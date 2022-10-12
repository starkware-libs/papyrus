use starknet_api::{DeclaredContract, StateNumber};

use crate::test_utils::{get_test_block, get_test_state_diff, get_test_storage};
use crate::{OmmerStorageWriter, StateStorageReader, StateStorageWriter, ThinStateDiff};

#[test]
fn move_state_diff_to_ommer() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let (header, _, state_diff, declared_classes) = get_test_state_diff();
    let block_number = header.block_number;
    let state_number = StateNumber::right_after_block(block_number);

    // Add state diff to cannonical tables.
    writer
        .begin_rw_txn()?
        .append_state_diff(block_number, state_diff, declared_classes)?
        .commit()?;

    // Get the state diff from the cannonical tables.
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

#[test]
fn insert_header_to_ommer() -> Result<(), anyhow::Error> {
    let (_, mut writer) = get_test_storage();
    let block = get_test_block(7);
    let block_hash = block.header.block_hash;

    writer.begin_rw_txn()?.insert_ommer_header(block_hash, &block.header)?.commit()?;

    Ok(())
}
