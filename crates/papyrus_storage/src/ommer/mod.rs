use starknet_api::{Block, BlockHash, DeclaredContract, StateDiff};

use crate::db::RW;
use crate::{StorageResult, StorageTxn};

#[cfg(test)]
#[path = "ommer_test.rs"]
mod ommer_test;

pub trait OmmerStorageWriter
where
    Self: Sized,
{
    // To enforce that no commit happen after a failure, we consume and return Self on success.
    fn insert_ommer_block(self, block_hash: BlockHash, block: Block) -> StorageResult<Self>;

    fn insert_ommer_state_diff(
        self,
        block_hash: BlockHash,
        state_diff: StateDiff,
        deployed_contract_class_definitions: Vec<DeclaredContract>,
    ) -> StorageResult<Self>;
}

impl<'env> OmmerStorageWriter for StorageTxn<'env, RW> {
    fn insert_ommer_block(self, block_hash: BlockHash, block: Block) -> StorageResult<Self> {
        self.txn.open_table(&self.tables.ommer_blocks)?.insert(&self.txn, &block_hash, &block)?;
        Ok(self)
    }

    fn insert_ommer_state_diff(
        self,
        block_hash: BlockHash,
        state_diff: StateDiff,
        deployed_contract_class_definitions: Vec<DeclaredContract>,
    ) -> StorageResult<Self> {
        self.txn.open_table(&self.tables.ommer_state_diffs)?.insert(
            &self.txn,
            &block_hash,
            &(state_diff, deployed_contract_class_definitions),
        )?;

        Ok(self)
    }
}
