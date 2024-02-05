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
    EventContent,
    EventIndexInTransactionOutput,
    ExecutionResources,
    Fee,
    MessageToL1,
    TransactionExecutionStatus,
    TransactionOutput,
};

use crate::body::{EventsTable, EventsTableKey, TransactionIndex};
use crate::db::serialization::{NoVersionValueWrapper, VersionZeroWrapper};
use crate::db::table_types::{DbCursor, DbCursorTrait, SimpleTable, Table};
use crate::db::{DbTransaction, RO};
use crate::{StorageResult, StorageTxn};

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

#[allow(missing_docs)]
/// A wrapper of two iterators [`EventIterByContractAddress`] and [`EventIterByEventIndex`].
pub enum EventIter<'txn, 'env> {
    ByContractAddress(EventIterByContractAddress<'txn>),
    ByEventIndex(EventIterByEventIndex<'txn, 'env>),
}

/// This iterator is a wrapper of two iterators [`EventIterByContractAddress`]
/// and [`EventIterByEventIndex`].
/// With this wrapper we can execute the same code, regardless the
/// type of iteration used.
impl Iterator for EventIter<'_, '_> {
    type Item = EventsTableKeyValue;

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
pub struct EventIterByContractAddress<'txn> {
    current: Option<EventsTableKeyValue>,
    cursor: EventsTableCursor<'txn>,
}

impl EventIterByContractAddress<'_> {
    /// Returns the next event. If there are no more events, returns None.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn next(&mut self) -> StorageResult<Option<EventsTableKeyValue>> {
        let res = self.current.take();
        self.current = self.cursor.next()?;
        Ok(res)
    }
}

/// This iterator goes over the events in the order of the event index.
/// That is, the events are iterated by the order they are emitted.
/// First by the block number, then by the transaction offset in the block,
/// and finally, by the event index in the transaction output.
pub struct EventIterByEventIndex<'txn, 'env> {
    txn: &'txn DbTransaction<'env, RO>,
    tx_current: Option<TransactionOutputsKeyValue>,
    tx_cursor: TransactionOutputsTableCursor<'txn>,
    events_table: EventsTable<'env>,
    event_index_in_tx_current: EventIndexInTransactionOutput,
    to_block_number: BlockNumber,
}

impl EventIterByEventIndex<'_, '_> {
    /// Returns the next event. If there are no more events, returns None.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn next(&mut self) -> StorageResult<Option<EventsTableKeyValue>> {
        let Some((tx_index, tx_output)) = &self.tx_current else { return Ok(None) };
        let Some(address) =
            tx_output.events_contract_addresses_as_ref().get(self.event_index_in_tx_current.0)
        else {
            return Ok(None);
        };
        let key = (*address, EventIndex(*tx_index, self.event_index_in_tx_current));
        let Some(content) = self.events_table.get(self.txn, &key)? else { return Ok(None) };
        self.event_index_in_tx_current.0 += 1;
        self.find_next_event_by_event_index()?;
        Ok(Some((key, content)))
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
            if tx_output.events_contract_addresses_as_ref().len() > self.event_index_in_tx_current.0
            {
                break;
            }

            // There are no more events in the current transaction, so we go over the rest of the
            // transactions until we find an event.
            self.tx_current = self.tx_cursor.next()?;
            self.event_index_in_tx_current = EventIndexInTransactionOutput(0);
        }

        Ok(())
    }
}

impl<'txn, 'env> StorageTxn<'env, RO> {
    /// Returns an events iterator that iterates events by the events table key from the given key.
    ///
    /// # Arguments
    /// * key - key to start from the first event with a key greater or equals to the given key.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn iter_events_by_contract_address(
        &'env self,
        key: EventsTableKey,
    ) -> StorageResult<EventIterByContractAddress<'txn>> {
        let events_table = self.open_table(&self.tables.events)?;
        let mut cursor = events_table.cursor(&self.txn)?;
        let current = cursor.lower_bound(&key)?;
        Ok(EventIterByContractAddress { current, cursor })
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
    ) -> StorageResult<EventIterByEventIndex<'txn, 'env>> {
        let transaction_outputs_table = self.open_table(&self.tables.transaction_outputs)?;
        let mut tx_cursor = transaction_outputs_table.cursor(&self.txn)?;
        let tx_current = tx_cursor.lower_bound(&event_index.0)?;
        let events_table = self.open_table(&self.tables.events)?;

        let mut it = EventIterByEventIndex {
            txn: &self.txn,
            tx_current,
            tx_cursor,
            events_table,
            event_index_in_tx_current: event_index.1,
            to_block_number,
        };
        it.find_next_event_by_event_index()?;
        Ok(it)
    }
}

#[allow(missing_docs)]
/// Each [`ThinTransactionOutput`] holds a list of event contract addresses so that given a thin
/// transaction output we can get all its events from the events table (see
/// [`get_transaction_events`](crate::body::BodyStorageReader::get_transaction_events) in
/// [`BodyStorageReader`](crate::body::BodyStorageReader)). These events contract addresses are
/// taken from the events in the order of the events in [`starknet_api`][`TransactionOutput`].
/// In particular, they are not sorted and with duplicates.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum ThinTransactionOutput {
    Declare(ThinDeclareTransactionOutput),
    Deploy(ThinDeployTransactionOutput),
    DeployAccount(ThinDeployAccountTransactionOutput),
    Invoke(ThinInvokeTransactionOutput),
    L1Handler(ThinL1HandlerTransactionOutput),
}

impl ThinTransactionOutput {
    /// Returns the events contract addresses of the transaction output.
    pub(crate) fn events_contract_addresses(self) -> Vec<ContractAddress> {
        match self {
            ThinTransactionOutput::Declare(tx_output) => tx_output.events_contract_addresses,
            ThinTransactionOutput::Deploy(tx_output) => tx_output.events_contract_addresses,
            ThinTransactionOutput::DeployAccount(tx_output) => tx_output.events_contract_addresses,
            ThinTransactionOutput::Invoke(tx_output) => tx_output.events_contract_addresses,
            ThinTransactionOutput::L1Handler(tx_output) => tx_output.events_contract_addresses,
        }
    }
    /// Returns the events contract addresses of the transaction output.
    pub(crate) fn events_contract_addresses_as_ref(&self) -> &Vec<ContractAddress> {
        match self {
            ThinTransactionOutput::Declare(tx_output) => &tx_output.events_contract_addresses,
            ThinTransactionOutput::Deploy(tx_output) => &tx_output.events_contract_addresses,
            ThinTransactionOutput::DeployAccount(tx_output) => &tx_output.events_contract_addresses,
            ThinTransactionOutput::Invoke(tx_output) => &tx_output.events_contract_addresses,
            ThinTransactionOutput::L1Handler(tx_output) => &tx_output.events_contract_addresses,
        }
    }
    /// Returns the execution status.
    pub fn execution_status(&self) -> &TransactionExecutionStatus {
        match self {
            ThinTransactionOutput::Declare(tx_output) => &tx_output.execution_status,
            ThinTransactionOutput::Deploy(tx_output) => &tx_output.execution_status,
            ThinTransactionOutput::DeployAccount(tx_output) => &tx_output.execution_status,
            ThinTransactionOutput::Invoke(tx_output) => &tx_output.execution_status,
            ThinTransactionOutput::L1Handler(tx_output) => &tx_output.execution_status,
        }
    }
    /// Returns the actual fee.
    pub fn actual_fee(&self) -> Fee {
        match self {
            ThinTransactionOutput::Declare(tx_output) => tx_output.actual_fee,
            ThinTransactionOutput::Deploy(tx_output) => tx_output.actual_fee,
            ThinTransactionOutput::DeployAccount(tx_output) => tx_output.actual_fee,
            ThinTransactionOutput::Invoke(tx_output) => tx_output.actual_fee,
            ThinTransactionOutput::L1Handler(tx_output) => tx_output.actual_fee,
        }
    }
}
/// A thin version of
/// [`InvokeTransactionOutput`](starknet_api::transaction::InvokeTransactionOutput), not holding the
/// events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinInvokeTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
    /// The execution status of the transaction.
    pub execution_status: TransactionExecutionStatus,
    /// The execution resources of the transaction.
    pub execution_resources: ExecutionResources,
}

/// A thin version of
/// [`L1HandlerTransactionOutput`](starknet_api::transaction::L1HandlerTransactionOutput), not
/// holding the events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinL1HandlerTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
    /// The execution status of the transaction.
    pub execution_status: TransactionExecutionStatus,
    /// The execution resources of the transaction.
    pub execution_resources: ExecutionResources,
}

/// A thin version of
/// [`DeclareTransactionOutput`](starknet_api::transaction::DeclareTransactionOutput), not holding
/// the events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinDeclareTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
    /// The execution status of the transaction.
    pub execution_status: TransactionExecutionStatus,
    /// The execution resources of the transaction.
    pub execution_resources: ExecutionResources,
}

/// A thin version of
/// [`DeployTransactionOutput`](starknet_api::transaction::DeployTransactionOutput), not holding the
/// events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinDeployTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
    /// The contract address of the deployed contract.
    pub contract_address: ContractAddress,
    /// The execution status of the transaction.
    pub execution_status: TransactionExecutionStatus,
    /// The execution resources of the transaction.
    pub execution_resources: ExecutionResources,
}

/// A thin version of
/// [`DeployAccountTransactionOutput`](starknet_api::transaction::DeployAccountTransactionOutput),
/// not holding the events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ThinDeployAccountTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
    /// The contract address of the deployed contract.
    pub contract_address: ContractAddress,
    /// The execution status of the transaction.
    pub execution_status: TransactionExecutionStatus,
    /// The execution resources of the transaction.
    pub execution_resources: ExecutionResources,
}

impl From<TransactionOutput> for ThinTransactionOutput {
    fn from(transaction_output: TransactionOutput) -> Self {
        let events_contract_addresses =
            transaction_output.events().iter().map(|event| event.from_address).collect();
        match transaction_output {
            TransactionOutput::Declare(tx_output) => {
                ThinTransactionOutput::Declare(ThinDeclareTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                    execution_status: tx_output.execution_status,
                    execution_resources: tx_output.execution_resources,
                })
            }
            TransactionOutput::Deploy(tx_output) => {
                ThinTransactionOutput::Deploy(ThinDeployTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                    contract_address: tx_output.contract_address,
                    execution_status: tx_output.execution_status,
                    execution_resources: tx_output.execution_resources,
                })
            }
            TransactionOutput::DeployAccount(tx_output) => {
                ThinTransactionOutput::DeployAccount(ThinDeployAccountTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                    contract_address: tx_output.contract_address,
                    execution_status: tx_output.execution_status,
                    execution_resources: tx_output.execution_resources,
                })
            }
            TransactionOutput::Invoke(tx_output) => {
                ThinTransactionOutput::Invoke(ThinInvokeTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                    execution_status: tx_output.execution_status,
                    execution_resources: tx_output.execution_resources,
                })
            }
            TransactionOutput::L1Handler(tx_output) => {
                ThinTransactionOutput::L1Handler(ThinL1HandlerTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                    execution_status: tx_output.execution_status,
                    execution_resources: tx_output.execution_resources,
                })
            }
        }
    }
}

/// A key-value pair of the events table.
type EventsTableKeyValue = (EventsTableKey, EventContent);
/// A cursor of the events table.
type EventsTableCursor<'txn> =
    DbCursor<'txn, RO, EventsTableKey, NoVersionValueWrapper<EventContent>, SimpleTable>;
/// A key-value pair of the transaction outputs table.
type TransactionOutputsKeyValue = (TransactionIndex, ThinTransactionOutput);
/// A cursor of the transaction outputs table.
type TransactionOutputsTableCursor<'txn> =
    DbCursor<'txn, RO, TransactionIndex, VersionZeroWrapper<ThinTransactionOutput>, SimpleTable>;
