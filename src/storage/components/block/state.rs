use crate::starknet::ContractAddress;
use crate::starknet::StarkFelt;
use crate::starknet::StateDiffForward;
use crate::starknet::{BlockHeader, BlockNumber};

use super::{BlockStorageError, BlockStorageReader, BlockStorageWriter, Result};

// Constants.
const STATE_MARKER_KEY: &[u8] = b"state";

pub trait StateStorageReader {
    fn get_state_marker(&self) -> Result<BlockNumber>;
    fn get_block_header(&self, block_number: BlockNumber) -> Result<Option<BlockHeader>>;
}
pub trait StateStorageWriter {
    fn append_state_diff(
        &mut self,
        block_number: BlockNumber,
        state_diff: &StateDiffForward,
    ) -> Result<()>;
}

impl StateStorageReader for BlockStorageReader {
    // The block number marker is the first block number that doesn't exist yet.
    fn get_state_marker(&self) -> Result<BlockNumber> {
        let txn = self.db_reader.begin_ro_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        Ok(txn
            .get::<BlockNumber>(&markers_table, STATE_MARKER_KEY)?
            .unwrap_or_default())
    }
    fn get_block_header(&self, block_number: BlockNumber) -> Result<Option<BlockHeader>> {
        let txn = self.db_reader.begin_ro_txn()?;
        let headers_table = txn.open_table(&self.tables.headers)?;
        let block_header =
            txn.get::<BlockHeader>(&headers_table, &bincode::serialize(&block_number).unwrap())?;
        Ok(block_header)
    }
}

impl StateStorageWriter for BlockStorageWriter {
    fn append_state_diff(
        &mut self,
        block_number: BlockNumber,
        state_diff: &StateDiffForward,
    ) -> Result<()> {
        let txn = self.db_writer.begin_rw_txn()?;
        let markers_table = txn.open_table(&self.tables.markers)?;
        let contracts_table = txn.open_table(&self.tables.contracts)?;
        let storage_table = txn.open_table(&self.tables.contract_storage)?;
        let state_diffs_table = txn.open_table(&self.tables.state_diffs)?;

        // Make sure marker is consistent.
        let state_marker = txn
            .get::<BlockNumber>(&markers_table, STATE_MARKER_KEY)?
            .unwrap_or_default();
        if state_marker != block_number {
            return Err(BlockStorageError::MarkerMismatch {
                expected: state_marker,
                found: block_number,
            });
        };

        // Advance marker.
        txn.upsert(&markers_table, STATE_MARKER_KEY, &block_number.next())?;

        // Prepare to build backwards state diff.
        let mut deployed_contracts: Vec<ContractAddress> = Vec::new();
        deployed_contracts.reserve(state_diff.deployed_contracts.len());

        // Update deployed contracts.
        for deployed_contract in &state_diff.deployed_contracts {
            deployed_contracts.push(deployed_contract.address);
            txn.insert(
                &contracts_table,
                &deployed_contract.address.0 .0,
                &deployed_contract.class_hash,
            )?;
        }

        // Update storage and build backwards state diff.
        let mut storage_diffs: Vec<(ContractAddress, Vec<(StarkFelt, StarkFelt)>)> = Vec::new();
        storage_diffs.reserve(state_diff.storage_diffs.len());
        for (address, forward_contract_diffs) in &state_diff.storage_diffs {
            let mut contract_diffs: Vec<(StarkFelt, StarkFelt)> = Vec::new();
            contract_diffs.reserve(forward_contract_diffs.len());
            for (key, value) in forward_contract_diffs {
                let db_key: &[u8] = &key.0;
                let prev_value = txn
                    .get::<StarkFelt>(&storage_table, db_key)?
                    .unwrap_or_default();
                contract_diffs.push((*key, prev_value));

                // Update DB.
                if value == &StarkFelt::default() {
                    txn.delete::<StarkFelt>(&storage_table, db_key)?;
                } else {
                    txn.upsert(&storage_table, db_key, value)?;
                }
            }
            storage_diffs.push((*address, contract_diffs));
        }

        // Write state diff.
        txn.insert(
            &state_diffs_table,
            &bincode::serialize(&block_number).unwrap(),
            &storage_diffs,
        )?;

        // Finalize.
        txn.commit()?;
        Ok(())
    }
}
