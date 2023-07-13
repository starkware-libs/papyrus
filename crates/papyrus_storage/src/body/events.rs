//! This module contains types and functions for iterating over events from the storage.
#[cfg(test)]
#[path = "events_test.rs"]
mod events_test;

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{
    EventContent, EventIndexInTransactionOutput, Fee, MessageToL1, TransactionOutput,
};

use crate::body::{EventsTable, EventsTableKey, TransactionIndex};
use crate::db::{DbCursor, DbTransaction, RO};
use crate::{StorageResult, StorageTxn};

/// An identifier of an event.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize, Serialize)]
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
        address: Option<ContractAddress>,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIter<'txn, 'env>> {
        if address.is_some() {
            return Ok(EventIter::ByContractAddress(
                self.iter_events_by_contract_address((address.unwrap(), event_index))?,
            ));
        }

        Ok(EventIter::ByEventIndex(self.iter_events_by_event_index(event_index, to_block_number)?))
    }
}

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
        let res = match self {
            EventIter::ByContractAddress(it) => it.next(),
            EventIter::ByEventIndex(it) => it.next(),
        };
        if res.is_err() {
            return None;
        }
        res.unwrap()
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
        let events_table = self.txn.open_table(&self.tables.events)?;
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
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let mut tx_cursor = transaction_outputs_table.cursor(&self.txn)?;
        let tx_current = tx_cursor.lower_bound(&event_index.0)?;
        let events_table = self.txn.open_table(&self.tables.events)?;

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

/// Each [`ThinTransactionOutput`] holds a list of event contract addresses so that given a thin
/// transaction output we can get all its events from the events table (see
/// [`get_transaction_events`](crate::body::BodyStorageReader::get_transaction_events) in
/// [`BodyStorageReader`](crate::body::BodyStorageReader)). These events contract addresses are
/// taken from the events in the order of the events in [`starknet_api`][`TransactionOutput`].
/// In particular, they are not sorted and with duplicates.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
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
}
/// A thin version of
/// [`InvokeTransactionOutput`](starknet_api::transaction::InvokeTransactionOutput), not holding the
/// events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinInvokeTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
}

/// A thin version of
/// [`L1HandlerTransactionOutput`](starknet_api::transaction::L1HandlerTransactionOutput), not
/// holding the events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinL1HandlerTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
}

/// A thin version of
/// [`DeclareTransactionOutput`](starknet_api::transaction::DeclareTransactionOutput), not holding
/// the events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinDeclareTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
}

/// A thin version of
/// [`DeployTransactionOutput`](starknet_api::transaction::DeployTransactionOutput), not holding the
/// events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinDeployTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
}

/// A thin version of
/// [`DeployAccountTransactionOutput`](starknet_api::transaction::DeployAccountTransactionOutput),
/// not holding the events content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinDeployAccountTransactionOutput {
    /// The actual fee paid for the transaction.
    pub actual_fee: Fee,
    /// The messages sent by the transaction to the base layer.
    pub messages_sent: Vec<MessageToL1>,
    /// The contract addresses of the events emitted by the transaction.
    pub events_contract_addresses: Vec<ContractAddress>,
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
                })
            }
            TransactionOutput::Deploy(tx_output) => {
                ThinTransactionOutput::Deploy(ThinDeployTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                })
            }
            TransactionOutput::DeployAccount(tx_output) => {
                ThinTransactionOutput::DeployAccount(ThinDeployAccountTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                })
            }
            TransactionOutput::Invoke(tx_output) => {
                ThinTransactionOutput::Invoke(ThinInvokeTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                })
            }
            TransactionOutput::L1Handler(tx_output) => {
                ThinTransactionOutput::L1Handler(ThinL1HandlerTransactionOutput {
                    actual_fee: tx_output.actual_fee,
                    messages_sent: tx_output.messages_sent,
                    events_contract_addresses,
                })
            }
        }
    }
}

/// A key-value pair of the events table.
type EventsTableKeyValue = (EventsTableKey, EventContent);
/// A cursor of the events table.
type EventsTableCursor<'txn> = DbCursor<'txn, RO, EventsTableKey, EventContent>;
/// A key-value pair of the transaction outputs table.
type TransactionOutputsKeyValue = (TransactionIndex, ThinTransactionOutput);
/// A cursor of the transaction outputs table.
type TransactionOutputsTableCursor<'txn> =
    DbCursor<'txn, RO, TransactionIndex, ThinTransactionOutput>;
