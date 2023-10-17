//! module for external utils, such as dumping a storage table to a file
#[cfg(test)]
#[path = "utils_test.rs"]
mod utils_test;

use std::fs::File;
use std::io::{BufWriter, Write};

use starknet_api::block::BlockNumber;

use crate::db::serialization::StorageSerde;
use crate::db::{DbIter, TableIdentifier, RO};
use crate::state::StateStorageReader;
use crate::{open_storage, StorageConfig, StorageResult, StorageTxn};

/// Dumps a table from the storage to a file in JSON format.
fn dump_table_to_file<K, V>(
    txn: &StorageTxn<'_, RO>,
    table_id: &TableIdentifier<K, V>,
    file_path: &str,
) -> StorageResult<()>
where
    K: StorageSerde + serde::Serialize,
    V: StorageSerde + serde::Serialize,
{
    let table_handle = txn.txn.open_table(table_id)?;
    let mut cursor = table_handle.cursor(&txn.txn)?;
    let iter = DbIter::new(&mut cursor);
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(b"[")?;
    let mut first = true;
    for data in iter {
        if !first {
            writer.write_all(b",")?;
        }
        serde_json::to_writer(&mut writer, &data?)?;
        first = false;
    }
    writer.write_all(b"]")?;
    Ok(())
}

/// Dumps the declared_classes table from the storage to a file.
pub fn dump_declared_classes_table_to_file(file_path: &str) -> StorageResult<()> {
    let storage_config = StorageConfig::default();
    let (storage_reader, _) = open_storage(storage_config)?;
    let txn = storage_reader.begin_ro_txn()?;
    dump_table_to_file(&txn, &txn.tables.declared_classes, file_path)?;
    Ok(())
}

/// Dumps the declared_classes at a given block range from the storage to a file.
pub fn dump_declared_classes_table_by_block_range(
    start_block: u64,
    end_block: u64,
    file_path: &str,
) -> StorageResult<()> {
    let storage_config = StorageConfig::default();
    let (storage_reader, _) = open_storage(storage_config)?;
    let txn = storage_reader.begin_ro_txn()?;
    let table_handle = txn.txn.open_table(&txn.tables.declared_classes)?;
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(b"[")?;
    let mut first = true;
    for block_number in start_block..=end_block {
        println!("block_number: {}", block_number);
        if let Some(thin_state_diff) = txn.get_state_diff(BlockNumber(block_number))? {
            for (class_hash, _) in thin_state_diff.declared_classes.iter() {
                if let Some(contract_class) = table_handle.get(&txn.txn, class_hash)? {
                    if !first {
                        writer.write_all(b",")?;
                    }
                    serde_json::to_writer(&mut writer, &(class_hash, &contract_class))?;
                    first = false;
                }
            }
        };
    }
    writer.write_all(b"]")?;
    Ok(())
}
