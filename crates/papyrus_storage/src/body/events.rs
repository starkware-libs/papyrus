//! Interface for iterating over events from the storage.
//!
//! Events are part of the transaction output. Each transaction output holds an array of events.
//! Import [`EventsReader`] to iterate over events using a read-only [`StorageTxn`].
//!
//! # Example
//! ```
//! use papyrus_storage::open_storage;
//! use papyrus_storage::body::TransactionIndex;
//! use papyrus_storage::body::events::{EventIndex, EventsReader};
//! # use papyrus_storage::{db::DbConfig, StorageConfig};
//! # use starknet_api::core::ChainId;
//! # use starknet_api::block::BlockNumber;
//! use starknet_api::core::ContractAddress;
//! use starknet_api::transaction::TransactionOffsetInBlock;
//! use starknet_api::transaction::EventIndexInTransactionOutput;
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! # let db_config = DbConfig {
//! #     path_prefix: dir,
//! #     chain_id: ChainId("SN_MAIN".to_owned()),
//! #     enforce_file_exists: false,
//! #     min_size: 1 << 20,    // 1MB
//! #     max_size: 1 << 35,    // 32GB
//! #     growth_step: 1 << 26, // 64MB
//! # };
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! // The API allows read-only interactions with the events. To write events, use the body writer.
//! let (reader, mut writer) = open_storage(storage_config)?;
//! // iterate events from all contracts, starting from the first event in the first transaction.
//! let event_index = EventIndex(
//!     TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)),
//!     EventIndexInTransactionOutput(0),
//! );
//! let txn = reader.begin_ro_txn()?; // The transaction must live longer than the iterator.
//! let events_iterator = txn.iter_events(None, event_index, BlockNumber(0))?;
//! for ((contract_address, event_index), event_content) in events_iterator {
//!    // Do something with the event.
//! }
//! // iterate events from a specific contract.
//! let contract_events_iterator = txn.iter_events(Some(ContractAddress::default()), event_index, BlockNumber(0))?;
//! for ((contract_address, event_index), event_content) in contract_events_iterator {
//!    // Do something with the event.
//! }
//! # Ok::<(), papyrus_storage::StorageError>(())
#[cfg(test)]
#[path = "events_test.rs"]
mod events_test;

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{
    Event,
    EventContent,
    EventIndexInTransactionOutput,
    TransactionOutput,
};
use tracing::error;

use super::TransactionMetadataTable;
use crate::body::{EventsTableKey, TransactionIndex};
use crate::db::serialization::{NoVersionValueWrapper, VersionZeroWrapper};
use crate::db::table_types::{DbCursor, DbCursorTrait, NoValue, SimpleTable, Table};
use crate::db::{DbTransaction, RO};
use crate::{FileHandlers, StorageResult, StorageTxn, TransactionMetadata};

/// An identifier of an event.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize, Serialize, PartialOrd, Ord)]
#[cfg_attr(any(test, feature = "testing"), derive(Hash))]
pub struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);

/// An interface for reading events.
pub trait EventsReader<'txn, 'env> {
    /// Returns an iterator over events, which is a wrapper of two iterators.
    /// If the address is none it iterates the events by the order of the event index,
    /// else, it iterated the events by the order of the contract addresses.
    ///
    /// # Arguments
    /// * address - contract address to iterate over events was emitted by it.
    /// * event_index - event index to start iterate from it.
    /// * to_block_number - block number to stop iterate at it.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn iter_events(
        &'env self,
        address: Option<ContractAddress>,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIter<'txn, 'env>>;
}

// TODO: support all read transactions (including RW).
impl<'txn, 'env> EventsReader<'txn, 'env> for StorageTxn<'env, RO> {
    fn iter_events(
        &'env self,
        optional_address: Option<ContractAddress>,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIter<'txn, 'env>> {
        if let Some(address) = optional_address {
            return Ok(EventIter::ByContractAddress(
                self.iter_events_by_contract_address((address, event_index))?,
            ));
        }

        Ok(EventIter::ByEventIndex(self.iter_events_by_event_index(event_index, to_block_number)?))
    }
}

// TODO(dvir): add transaction hash to the return value. In the RPC when returning events this is
// with the transaction hash. We can do it efficiently here because we anyway read the relevant
// entry in the transaction_metadata table..
#[allow(missing_docs)]
/// A wrapper of two iterators [`EventIterByContractAddress`] and [`EventIterByEventIndex`].
pub enum EventIter<'txn, 'env> {
    ByContractAddress(EventIterByContractAddress<'env, 'txn>),
    ByEventIndex(EventIterByEventIndex<'txn>),
}

/// This iterator is a wrapper of two iterators [`EventIterByContractAddress`]
/// and [`EventIterByEventIndex`].
/// With this wrapper we can execute the same code, regardless the
/// type of iteration used.
impl Iterator for EventIter<'_, '_> {
    type Item = ((ContractAddress, EventIndex), EventContent);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            EventIter::ByContractAddress(it) => it.next(),
            EventIter::ByEventIndex(it) => it.next(),
        }
        .unwrap_or(None)
    }
}

/// This iterator goes over the events in the order of the events table key.
/// That is, the events iterated first by the contract address and then by the event index.
pub struct EventIterByContractAddress<'env, 'txn> {
    txn: &'txn DbTransaction<'env, RO>,
    file_handles: &'txn FileHandlers<RO>,
    // This value is the next event to return. If it is None there are no more events.
    current: Option<EventsTableKey>,
    // The current transaction output. This is None only at the beginning of the iteration and
    // filled with the first transaction output.
    current_tx: Option<(TransactionIndex, TransactionOutput)>,
    cursor: EventsTableCursor<'txn>,
    transaction_metadata_table: TransactionMetadataTable<'env>,
    current_event_index: usize,
    current_contract_address: ContractAddress,
}

impl<'env, 'txn> EventIterByContractAddress<'env, 'txn> {
    /// Returns the next event. If there are no more events, returns None.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn next(&mut self) -> StorageResult<Option<((ContractAddress, EventIndex), EventContent)>> {
        let mut should_get_next_tx = true;

        if self.current_tx.is_some() {
            let events = self.current_tx.as_ref().expect("current_tx is some.").1.events();
            if let Some(next_event_index) = (self.current_event_index..events.len())
                .find(|&idx| events[idx].from_address == self.current_contract_address)
            {
                self.current_event_index = next_event_index;
                should_get_next_tx = false;
            }
        }

        if should_get_next_tx {
            let Some((contract_address, tx_index)) = self.current.take() else {
                return Ok(None);
            };
            let Some(tx_metadata) = self.transaction_metadata_table.get(self.txn, &tx_index)?
            else {
                error!("Transaction metadata not found for transaction index: {tx_index:?}");
                return Ok(None);
            };
            self.current_tx = Some((
                tx_index,
                self.file_handles
                    .get_transaction_output_unchecked(tx_metadata.tx_output_location)?,
            ));
            self.current_event_index = 0;
            self.current_contract_address = contract_address;
            self.current = self.cursor.next()?.map(|(key, _)| key);
        }

        let (tx_index, tx_output) = self.current_tx.as_ref().expect("current_tx was initialized");
        let events = tx_output.events();

        let Some(event_offset) = (self.current_event_index..events.len())
            .find(|&idx| events[idx].from_address == self.current_contract_address)
        else {
            error!(
                "Event not found for contract address: {:?} in transaction: {tx_index:?}",
                self.current_contract_address
            );
            return Ok(None);
        };

        let key = (
            self.current_contract_address,
            EventIndex(*tx_index, EventIndexInTransactionOutput(event_offset)),
        );
        // TODO(dvir): don't clone here the event content.
        let content = self
            .current_tx
            .as_ref()
            .expect(
                "The current transaction was initialized with Some previously in this function.",
            )
            .1
            .events()[event_offset]
            .content
            .clone();

        self.current_event_index = event_offset + 1;
        Ok(Some((key, content)))
    }
}

/// This iterator goes over the events in the order of the event index.
/// That is, the events are iterated by the order they are emitted.
/// First by the block number, then by the transaction offset in the block,
/// and finally, by the event index in the transaction output.
pub struct EventIterByEventIndex<'txn> {
    file_handlers: &'txn FileHandlers<RO>,
    tx_current: Option<(TransactionIndex, TransactionOutput)>,
    tx_cursor: TransactionMetadataTableCursor<'txn>,
    event_index_in_tx_current: EventIndexInTransactionOutput,
    to_block_number: BlockNumber,
}

impl EventIterByEventIndex<'_> {
    /// Returns the next event. If there are no more events, returns None.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn next(&mut self) -> StorageResult<Option<((ContractAddress, EventIndex), EventContent)>> {
        let Some((tx_index, tx_output)) = &self.tx_current else { return Ok(None) };
        let Some(Event { from_address, content }) =
            tx_output.events().get(self.event_index_in_tx_current.0)
        else {
            return Ok(None);
        };
        let key = (*from_address, EventIndex(*tx_index, self.event_index_in_tx_current));
        // TODO(dvir): don't clone here the event content.
        let content = content.clone();
        self.event_index_in_tx_current.0 += 1;
        self.find_next_event_by_event_index()?;
        Ok(Some((key, content.clone())))
    }

    /// Finds the event that corresponds to the first event index greater than or equals to the
    /// current event index. The current event index is composed of the transaction index of the
    /// current transaction (tx_current) and the event index in current transaction output
    /// (event_index_in_tx_current).
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn find_next_event_by_event_index(&mut self) -> StorageResult<()> {
        while let Some((tx_index, tx_output)) = &self.tx_current {
            if tx_index.0 > self.to_block_number {
                self.tx_current = None;
                break;
            }
            // Checks if there's an event in the current event index.
            if tx_output.events().len() > self.event_index_in_tx_current.0 {
                break;
            }

            // There are no more events in the current transaction, so we go over the rest of the
            // transactions until we find an event.
            let Some((tx_index, tx_metadata)) = self.tx_cursor.next()? else {
                self.tx_current = None;
                return Ok(());
            };
            self.tx_current = Some((
                tx_index,
                self.file_handlers
                    .get_transaction_output_unchecked(tx_metadata.tx_output_location)?,
            ));
            self.event_index_in_tx_current = EventIndexInTransactionOutput(0);
        }

        Ok(())
    }
}

impl<'txn, 'env> StorageTxn<'env, RO>
where
    'env: 'txn,
{
    /// Returns an events iterator that iterates events by the events table key from the given key.
    ///
    /// # Arguments
    /// * key - key to start from the first event with a key greater or equals to the given key.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn iter_events_by_contract_address(
        &'env self,
        key: (ContractAddress, EventIndex),
    ) -> StorageResult<EventIterByContractAddress<'env, 'txn>> {
        let transaction_metadata_table = self.open_table(&self.tables.transaction_metadata)?;
        let events_table = self.open_table(&self.tables.events)?;
        let mut cursor = events_table.cursor(&self.txn)?;
        let current = cursor.lower_bound(&(key.0, key.1.0))?.map(|(key, _)| key);
        Ok(EventIterByContractAddress {
            txn: &self.txn,
            file_handles: &self.file_handlers,
            current,
            current_tx: None,
            cursor,
            transaction_metadata_table,
            current_event_index: key.1.1.0,
            current_contract_address: key.0,
        })
    }

    /// Returns an events iterator that iterates events by event index from the given event index.
    ///
    /// # Arguments
    /// * event_index - event index to start from the first event with an index greater or equals
    ///   to.
    /// * to_block_number - block number to stop iterate at it.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn iter_events_by_event_index(
        &'env self,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIterByEventIndex<'txn>> {
        let transaction_metadata_table = self.open_table(&self.tables.transaction_metadata)?;
        let mut tx_cursor = transaction_metadata_table.cursor(&self.txn)?;
        let first_txn_location = tx_cursor.lower_bound(&event_index.0)?;
        let first_relevant_transaction = match first_txn_location {
            None => None,
            Some((tx_index, tx_metadata)) => Some((
                tx_index,
                self.file_handlers
                    .get_transaction_output_unchecked(tx_metadata.tx_output_location)?,
            )),
        };

        let mut it = EventIterByEventIndex {
            file_handlers: &self.file_handlers,
            tx_current: first_relevant_transaction,
            tx_cursor,
            event_index_in_tx_current: event_index.1,
            to_block_number,
        };
        it.find_next_event_by_event_index()?;
        Ok(it)
    }
}

/// A cursor of the events table.
type EventsTableCursor<'txn> =
    DbCursor<'txn, RO, EventsTableKey, NoVersionValueWrapper<NoValue>, SimpleTable>;
/// A cursor of the transaction outputs table.
type TransactionMetadataTableCursor<'txn> =
    DbCursor<'txn, RO, TransactionIndex, VersionZeroWrapper<TransactionMetadata>, SimpleTable>;
