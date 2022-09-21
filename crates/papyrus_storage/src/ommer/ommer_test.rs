use starknet_api::{shash, BlockBody, BlockHash, BlockHeader, StarkHash};

use super::OmmerStorageWriter;
use crate::test_utils::get_test_storage;
use crate::{StorageError, StorageReader, StorageResult, ThinStateDiff};

fn get_ommer_header(
    reader: &StorageReader,
    block_hash: BlockHash,
) -> StorageResult<Option<BlockHeader>> {
    let storage_txn = reader.begin_ro_txn()?;
    let ommer_headers_table = storage_txn.txn.open_table(&storage_txn.tables.ommer_headers)?;
    ommer_headers_table
        .get(&storage_txn.txn, &block_hash)
        .map_err(StorageError::InnerError)
}

#[test]
fn add_ommer() -> Result<(), anyhow::Error> {
    let (reader, mut writer) = get_test_storage();
    let hash = BlockHash::new(shash!("0x0"));

    assert!(get_ommer_header(&reader, hash)?.is_none());

    writer
        .begin_rw_txn()?
        .insert_ommer_block(
            hash,
            &BlockHeader::default(),
            &BlockBody::default(),
            &ThinStateDiff::default(),
        )?
        .commit()?;

    assert!(get_ommer_header(&reader, hash)?.is_some());
    Ok(())
}
