use starknet_api::{
    BlockHash, BlockHeader, DeclaredContract, Event, EventIndexInTransactionOutput, Transaction,
    TransactionOffsetInBlock,
};

#[cfg(test)]
#[path = "ommer_test.rs"]
mod ommer_test;

use crate::db::RW;
use crate::{
    OmmerEventKey, OmmerTransactionKey, StorageResult, StorageTxn, ThinStateDiff,
    ThinTransactionOutput,
};

/// Writer for ommer blocks data.
/// To enforce that no commit happen after a failure, we consume and return Self on success.
pub trait OmmerStorageWriter
where
    Self: Sized,
{
    fn insert_ommer_header(
        self,
        block_hash: BlockHash,
        header: &BlockHeader,
    ) -> StorageResult<Self>;

    fn insert_ommer_body(
        self,
        block_hash: BlockHash,
        transactions: &[Transaction],
        thin_transaction_outputs: &[ThinTransactionOutput],
        transaction_outputs_events: &[Vec<Event>],
    ) -> StorageResult<Self>;

    fn insert_ommer_state_diff(
        self,
        block_hash: BlockHash,
        thin_state_diff: &ThinStateDiff,
        declared_classes: &[DeclaredContract],
    ) -> StorageResult<Self>;
}

impl<'env> OmmerStorageWriter for StorageTxn<'env, RW> {
    fn insert_ommer_header(
        self,
        block_hash: BlockHash,
        header: &BlockHeader,
    ) -> StorageResult<Self> {
        let ommer_headers_table = self.txn.open_table(&self.tables.ommer_headers)?;
        ommer_headers_table.insert(&self.txn, &block_hash, header)?;

        Ok(self)
    }

    fn insert_ommer_body(
        self,
        block_hash: BlockHash,
        transactions: &[Transaction],
        thin_transaction_outputs: &[ThinTransactionOutput],
        transaction_outputs_events: &[Vec<Event>],
    ) -> StorageResult<Self> {
        assert!(transactions.len() == thin_transaction_outputs.len());
        assert!(transactions.len() == transaction_outputs_events.len());

        let ommer_transactions_table = self.txn.open_table(&self.tables.ommer_transactions)?;
        let ommer_transaction_outputs_table =
            self.txn.open_table(&self.tables.ommer_transaction_outputs)?;
        let ommer_events_table = self.txn.open_table(&self.tables.ommer_events)?;

        for idx in 0..transactions.len() {
            let tx_index = OmmerTransactionKey(block_hash, TransactionOffsetInBlock(idx));
            ommer_transactions_table.insert(&self.txn, &tx_index, &transactions[idx])?;
            ommer_transaction_outputs_table.insert(
                &self.txn,
                &tx_index,
                &thin_transaction_outputs[idx],
            )?;
            let events = &transaction_outputs_events[idx];
            for (event_offset, event) in events.iter().enumerate() {
                let event_index =
                    OmmerEventKey(tx_index, EventIndexInTransactionOutput(event_offset));
                ommer_events_table.insert(
                    &self.txn,
                    &(event.from_address, event_index),
                    &event.content,
                )?;
            }
        }

        Ok(self)
    }
    fn insert_ommer_state_diff(
        self,
        block_hash: BlockHash,
        thin_state_diff: &ThinStateDiff,
        declared_classes: &[DeclaredContract],
    ) -> StorageResult<Self> {
        let ommer_state_diffs_table = self.txn.open_table(&self.tables.ommer_state_diffs)?;
        let ommer_declared_classes_table =
            self.txn.open_table(&self.tables.ommer_declared_classes)?;
        let ommer_deployed_contracts_table =
            self.txn.open_table(&self.tables.ommer_deployed_contracts)?;
        let ommer_storage_table = self.txn.open_table(&self.tables.ommer_contract_storage)?;
        let ommer_nonces_table = self.txn.open_table(&self.tables.ommer_nonces)?;

        ommer_state_diffs_table.insert(&self.txn, &block_hash, thin_state_diff)?;

        for declared_class in declared_classes {
            let key = (block_hash, declared_class.class_hash);
            let value = declared_class.contract_class.to_byte_vec();
            ommer_declared_classes_table.insert(&self.txn, &key, &value)?;
        }

        for deployed_contract in thin_state_diff.deployed_contracts() {
            let key = (deployed_contract.address, block_hash);
            let value = deployed_contract.class_hash;
            ommer_deployed_contracts_table.insert(&self.txn, &key, &value)?;
        }

        for storage_diff in thin_state_diff.storage_diffs() {
            for storage_entry in &storage_diff.storage_entries {
                let key = (storage_diff.address, storage_entry.key.clone(), block_hash);
                let value = storage_entry.value;
                ommer_storage_table.insert(&self.txn, &key, &value)?;
            }
        }

        for contract_nonce in thin_state_diff.nonces() {
            let key = (contract_nonce.contract_address, block_hash);
            let value = contract_nonce.nonce;
            ommer_nonces_table.insert(&self.txn, &key, &value)?;
        }

        Ok(self)
    }
}
