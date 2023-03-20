#[cfg(test)]
#[path = "ommer_test.rs"]
mod ommer_test;

use indexmap::IndexMap;
use starknet_api::block::{BlockHash, BlockHeader};
use starknet_api::core::ClassHash;
use starknet_api::state::ContractClass;
use starknet_api::transaction::{
    EventContent, EventIndexInTransactionOutput, Transaction, TransactionOffsetInBlock,
};

use crate::body::events::ThinTransactionOutput;
use crate::db::{DbError, RW};
use crate::state::data::ThinStateDiff;
use crate::{
    OmmerEventKey, OmmerTransactionKey, StorageError, StorageResult, StorageTxn, TransactionKind,
};

pub trait OmmerStorageReader {
    fn get_ommer_header(&self, block_hash: BlockHash) -> StorageResult<Option<BlockHeader>>;
}

impl<'env, Mode: TransactionKind> OmmerStorageReader for StorageTxn<'env, Mode> {
    fn get_ommer_header(&self, block_hash: BlockHash) -> StorageResult<Option<BlockHeader>> {
        self.txn
            .open_table(&self.tables.ommer_headers)?
            .get(&self.txn, &block_hash)
            .map_err(StorageError::InnerError)
    }
}

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
        transaction_outputs_events: &[Vec<EventContent>],
    ) -> StorageResult<Self>;

    fn insert_ommer_state_diff(
        self,
        block_hash: BlockHash,
        thin_state_diff: &ThinStateDiff,
        declared_classes: &IndexMap<ClassHash, ContractClass>,
    ) -> StorageResult<Self>;
}

impl<'env> OmmerStorageWriter for StorageTxn<'env, RW> {
    fn insert_ommer_header(
        self,
        block_hash: BlockHash,
        header: &BlockHeader,
    ) -> StorageResult<Self> {
        let ommer_headers_table = self.txn.open_table(&self.tables.ommer_headers)?;
        ommer_headers_table.insert(&self.txn, &block_hash, header).map_err(|err| match err {
            DbError::Inner(libmdbx::Error::KeyExist) => {
                StorageError::OmmerHeaderAlreadyExists { block_hash }
            }
            err => err.into(),
        })?;

        Ok(self)
    }

    fn insert_ommer_body(
        self,
        block_hash: BlockHash,
        transactions: &[Transaction],
        thin_transaction_outputs: &[ThinTransactionOutput],
        transaction_outputs_events: &[Vec<EventContent>],
    ) -> StorageResult<Self> {
        assert!(transactions.len() == thin_transaction_outputs.len());
        assert!(transactions.len() == transaction_outputs_events.len());

        let ommer_transactions_table = self.txn.open_table(&self.tables.ommer_transactions)?;
        let ommer_transaction_outputs_table =
            self.txn.open_table(&self.tables.ommer_transaction_outputs)?;
        let ommer_events_table = self.txn.open_table(&self.tables.ommer_events)?;

        for idx in 0..transactions.len() {
            let tx_index = OmmerTransactionKey(block_hash, TransactionOffsetInBlock(idx));
            ommer_transactions_table.insert(&self.txn, &tx_index, &transactions[idx]).map_err(
                |err| match err {
                    DbError::Inner(libmdbx::Error::KeyExist) => {
                        StorageError::OmmerTransactionKeyAlreadyExists { tx_key: tx_index }
                    }
                    err => err.into(),
                },
            )?;
            ommer_transaction_outputs_table
                .insert(&self.txn, &tx_index, &thin_transaction_outputs[idx])
                .map_err(|err| match err {
                    DbError::Inner(libmdbx::Error::KeyExist) => {
                        StorageError::OmmerTransactionOutputKeyAlreadyExists { tx_key: tx_index }
                    }
                    err => err.into(),
                })?;
            let events = &transaction_outputs_events[idx];
            for (event_offset, (event, address)) in events
                .iter()
                .zip(thin_transaction_outputs[idx].events_contract_addresses_as_ref().iter())
                .enumerate()
            {
                let event_key =
                    OmmerEventKey(tx_index, EventIndexInTransactionOutput(event_offset));
                ommer_events_table.insert(&self.txn, &(*address, event_key), event).map_err(
                    |err| match err {
                        DbError::Inner(libmdbx::Error::KeyExist) => {
                            StorageError::OmmerEventAlreadyExists {
                                contract_address: *address,
                                event_key,
                            }
                        }
                        err => err.into(),
                    },
                )?;
            }
        }

        Ok(self)
    }

    fn insert_ommer_state_diff(
        self,
        block_hash: BlockHash,
        thin_state_diff: &ThinStateDiff,
        declared_classes: &IndexMap<ClassHash, ContractClass>,
    ) -> StorageResult<Self> {
        let ommer_state_diffs_table = self.txn.open_table(&self.tables.ommer_state_diffs)?;
        let ommer_declared_classes_table =
            self.txn.open_table(&self.tables.ommer_declared_classes)?;
        let ommer_deployed_contracts_table =
            self.txn.open_table(&self.tables.ommer_deployed_contracts)?;
        let ommer_storage_table = self.txn.open_table(&self.tables.ommer_contract_storage)?;
        let ommer_nonces_table = self.txn.open_table(&self.tables.ommer_nonces)?;

        ommer_state_diffs_table.insert(&self.txn, &block_hash, thin_state_diff).map_err(|err| {
            match err {
                DbError::Inner(libmdbx::Error::KeyExist) => {
                    StorageError::OmmerStateDiffAlreadyExists { block_hash }
                }
                err => err.into(),
            }
        })?;

        for (class_hash, contract_class) in declared_classes {
            let key = (block_hash, *class_hash);
            let value = contract_class;
            ommer_declared_classes_table.insert(&self.txn, &key, value).map_err(
                |err| match err {
                    DbError::Inner(libmdbx::Error::KeyExist) => {
                        StorageError::OmmerClassAlreadyExists {
                            block_hash,
                            class_hash: *class_hash,
                        }
                    }
                    err => err.into(),
                },
            )?;
        }

        for (address, class_hash) in &thin_state_diff.deployed_contracts {
            let key = (*address, block_hash);
            let value = class_hash;
            ommer_deployed_contracts_table.insert(&self.txn, &key, value).map_err(
                |err| match err {
                    DbError::Inner(libmdbx::Error::KeyExist) => {
                        StorageError::OmmerDeployedContractAlreadyExists {
                            block_hash,
                            contract_address: *address,
                        }
                    }
                    err => err.into(),
                },
            )?;
        }

        for (address, storage_entries) in &thin_state_diff.storage_diffs {
            for (storage_key, value) in storage_entries {
                let key = (*address, *storage_key, block_hash);
                ommer_storage_table.insert(&self.txn, &key, value).map_err(|err| match err {
                    DbError::Inner(libmdbx::Error::KeyExist) => {
                        StorageError::OmmerStorageKeyAlreadyExists {
                            block_hash,
                            contract_address: *address,
                            key: *storage_key,
                        }
                    }
                    err => err.into(),
                })?;
            }
        }

        for (contract_address, nonce) in &thin_state_diff.nonces {
            let key = (*contract_address, block_hash);
            let value = nonce;
            ommer_nonces_table.insert(&self.txn, &key, value).map_err(|err| match err {
                DbError::Inner(libmdbx::Error::KeyExist) => StorageError::OmmerNonceAlreadyExists {
                    block_hash,
                    contract_address: *contract_address,
                },
                err => err.into(),
            })?;
        }

        Ok(self)
    }
}
