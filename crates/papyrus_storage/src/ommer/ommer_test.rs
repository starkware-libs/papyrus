use starknet_api::{shash, Block, BlockHash, DeclaredContract, StarkHash, StateDiff};

use super::OmmerStorageWriter;
use crate::test_utils::{get_test_block, get_test_state_diff, get_test_storage};
use crate::StorageReader;

fn get_ommer_block(reader: &StorageReader, block_hash: BlockHash) -> Option<Block> {
    let storage_txn = reader.begin_ro_txn().unwrap();
    storage_txn
        .txn
        .open_table(&reader.tables.ommer_blocks)
        .unwrap()
        .get(&storage_txn.txn, &block_hash)
        .unwrap()
}

fn get_ommer_state_diff(
    reader: &StorageReader,
    block_hash: BlockHash,
) -> Option<(StateDiff, Vec<DeclaredContract>)> {
    let storage_txn = reader.begin_ro_txn().unwrap();
    storage_txn
        .txn
        .open_table(&reader.tables.ommer_state_diffs)
        .unwrap()
        .get(&storage_txn.txn, &block_hash)
        .unwrap()
        .map(|serialized| serialized.try_into().unwrap())
}

#[test]
fn insert_ommer_block() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let hash = BlockHash::new(shash!("0x0"));
    assert!(get_ommer_block(&reader, hash).is_none());

    let block = get_test_block(7);
    writer.begin_rw_txn()?.insert_ommer_block(hash, block.clone())?.commit()?;

    if let Some(ommer_block) = get_ommer_block(&reader, hash) {
        assert_eq!(ommer_block, block);
    } else {
        panic!("Unexpected none")
    }

    Ok(())
}

#[test]
fn insert_ommer_state_diff() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let hash = BlockHash::new(shash!("0x0"));

    assert!(get_ommer_state_diff(&reader, hash).is_none());

    let (_, _, state_diff, declared_contracts) = get_test_state_diff();
    writer
        .begin_rw_txn()?
        .insert_ommer_state_diff(hash, state_diff.clone(), declared_contracts.clone())?
        .commit()?;

    if let Some((ommer_state_diff, ommer_declared_contracts)) = get_ommer_state_diff(&reader, hash)
    {
        assert_eq!(ommer_state_diff, state_diff);
        assert_eq!(ommer_declared_contracts, declared_contracts);
    } else {
        panic!("Unexpected none")
    }
    Ok(())
}
