//! module for external utils, such as dumping a storage table to a file
#[cfg(test)]
#[path = "utils_test.rs"]
mod utils_test;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};

use serde::Serialize;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{EntryPoint, EntryPointType};

use crate::compiled_class::CasmStorageReader;
use crate::db::table_types::Table;
use crate::db::RO;
use crate::state::StateStorageReader;
use crate::{open_storage, StorageConfig, StorageError, StorageResult, StorageTxn};

#[derive(Serialize)]
struct DumpDeclaredClass {
    class_hash: ClassHash,
    compiled_class_hash: CompiledClassHash,
    sierra_program: Vec<StarkFelt>,
    entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

/// Dumps the declared_classes at a given block range from the storage to a file.
pub fn dump_declared_classes_table_by_block_range(
    start_block: u64,
    end_block: u64,
    file_path: &str,
    chain_id: &str,
) -> StorageResult<()> {
    let mut storage_config = StorageConfig::default();
    storage_config.db_config.chain_id = ChainId(chain_id.to_string());
    let (storage_reader, _) = open_storage(storage_config)?;
    let txn = storage_reader.begin_ro_txn()?;
    let compiled_class_marker = txn.get_compiled_class_marker()?;
    if end_block > compiled_class_marker.0 {
        return Err(StorageError::InvalidBlockNumber {
            block: BlockNumber(end_block),
            compiled_class_marker,
        });
    }
    dump_declared_classes_table_by_block_range_internal(&txn, file_path, start_block, end_block)
}

fn dump_declared_classes_table_by_block_range_internal(
    txn: &StorageTxn<'_, RO>,
    file_path: &str,
    start_block: u64,
    end_block: u64,
) -> StorageResult<()> {
    let table_handle = txn.txn.open_table(&txn.tables.declared_classes)?;
    let file = File::create(file_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(b"[")?;
    let mut first = true;
    for block_number in start_block..end_block {
        if let Some(thin_state_diff) = txn.get_state_diff(BlockNumber(block_number))? {
            for (class_hash, compiled_class_hash) in thin_state_diff.declared_classes.iter() {
                if let Some(contract_class_location) = table_handle.get(&txn.txn, class_hash)? {
                    let contract_class =
                        txn.file_handlers.get_contract_class_unchecked(contract_class_location)?;
                    if !first {
                        writer.write_all(b",")?;
                    }
                    serde_json::to_writer(
                        &mut writer,
                        &DumpDeclaredClass {
                            class_hash: *class_hash,
                            compiled_class_hash: *compiled_class_hash,
                            sierra_program: contract_class.sierra_program.clone(),
                            entry_points_by_type: contract_class.entry_point_by_type.clone(),
                        },
                    )?;
                    first = false;
                }
            }
        };
    }
    writer.write_all(b"]")?;
    Ok(())
}
