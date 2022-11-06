#[cfg(test)]
#[path = "events_test.rs"]
mod events_test;

use serde::{Deserialize, Serialize};
use starknet_api::{
    BlockNumber, ContractAddress, EventContent, EventIndexInTransactionOutput, Fee, MessageToL1,
    TransactionOutput,
};

use crate::db::{DbCursor, DbTransaction, TableHandle, RO};
use crate::{EventIndex, StorageResult, StorageTxn, TransactionIndex};

// EventIndex is a tuple:
// (block number, transaction offset in block, event index in transaction output).
// This is the order of the events as they are emitted.
pub type EventsTableKey = (ContractAddress, EventIndex);
pub type EventsTableKeyValue = (EventsTableKey, EventContent);
pub type EventsTableCursor<'txn> = DbCursor<'txn, RO, EventsTableKey, EventContent>;
pub type EventsTable<'env> = TableHandle<'env, EventsTableKey, EventContent>;
type TransactionOutputsKeyValue = (TransactionIndex, ThinTransactionOutput);
type TransactionOutputsTableCursor<'txn> =
    DbCursor<'txn, RO, TransactionIndex, ThinTransactionOutput>;

pub enum EventIter<'txn, 'env> {
    Key(EventsTableKeyIter<'txn>),
    Index(EventIndexIter<'txn, 'env>),
}

/// This iterator is a wrapper of two iterators [``] and [``].
/// With this wrapper we can execute the same code, regardless the
/// type of iteration used.
impl Iterator for EventIter<'_, '_> {
    type Item = EventsTableKeyValue;

    fn next(&mut self) -> Option<Self::Item> {
        let res = match self {
            EventIter::Key(it) => it.next(),
            EventIter::Index(it) => it.next(),
        };
        if res.is_err() {
            return None;
        }
        res.unwrap()
    }
}

/// This iterator goes over the events in the order of the events table key.
/// That is, the events iterated first by the contract address and then by the event index.
pub struct EventsTableKeyIter<'txn> {
    current: Option<EventsTableKeyValue>,
    cursor: EventsTableCursor<'txn>,
}

impl EventsTableKeyIter<'_> {
    pub fn next(&mut self) -> StorageResult<Option<EventsTableKeyValue>> {
        let res = self.current.take();
        self.current = self.cursor.next()?;
        Ok(res)
    }
}

/// This iterator goes over the events in the order of the event index.
/// That is, the events are iterated by the order they are emitted.
/// First by the block number, then by the transaction offset in the block,
/// and finally, by the event index in the transaction output.
pub struct EventIndexIter<'txn, 'env> {
    txn: &'txn DbTransaction<'env, RO>,
    tx_current: Option<TransactionOutputsKeyValue>,
    tx_cursor: TransactionOutputsTableCursor<'txn>,
    events_table: EventsTable<'env>,
    current: Option<EventsTableKeyValue>,
    to_block_number: BlockNumber,
}

impl EventIndexIter<'_, '_> {
    fn get_event(
        &self,
        tx_index: &TransactionIndex,
        event_index_in_tx: &EventIndexInTransactionOutput,
        tx_output: &ThinTransactionOutput,
    ) -> StorageResult<Option<EventsTableKeyValue>> {
        if let Some(address) =
            tx_output.events_contract_addresses_as_ref().iter().nth(event_index_in_tx.0)
        {
            let key = (*address, EventIndex(*tx_index, *event_index_in_tx));
            if let Some(content) = self.events_table.get(self.txn, &key)? {
                return Ok(Some((key, content)));
            }
        }

        Ok(None)
    }

    /// Returns a key-value pair that corresponds to the first index greater than or equal to the
    /// specified index.
    fn greater_or_equal(
        &mut self,
        event_index_in_tx: &EventIndexInTransactionOutput,
    ) -> StorageResult<Option<EventsTableKeyValue>> {
        // Check the specified index. If there's an event there return it.
        if let Some((tx_index, tx_output)) = &self.tx_current {
            if let Some(item) = self.get_event(tx_index, event_index_in_tx, tx_output)? {
                return Ok(Some(item));
            };
        }

        // There are no more events in the specified transaction, so we go over the rest of the
        // transactions until we find an event.
        self.tx_current = self.tx_cursor.next()?;
        while let Some((tx_index, tx_output)) = &self.tx_current {
            if tx_index.0 > self.to_block_number {
                break;
            }
            if let Some(item) =
                self.get_event(tx_index, &EventIndexInTransactionOutput(0), tx_output)?
            {
                return Ok(Some(item));
            }
            self.tx_current = self.tx_cursor.next()?;
        }

        Ok(None)
    }

    pub fn next(&mut self) -> StorageResult<Option<EventsTableKeyValue>> {
        match self.current {
            None => Ok(None),
            Some((key, _)) => {
                let res = self.current.take();
                let current_event_index_in_tx = (key.1).1;
                self.current = self.greater_or_equal(&current_event_index_in_tx.next())?;
                Ok(res)
            }
        }
    }
}

pub trait EventsReader<'txn, 'env> {
    fn iter_events(
        &'env self,
        address: Option<ContractAddress>,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIter<'txn, 'env>>;
}

impl<'txn, 'env> StorageTxn<'env, RO> {
    /// Returns an events iterator that iterates events by the events table key,
    /// starting from the first event with a key greater or equal to the given key.
    fn iter_events_by_key(
        &'env self,
        key: EventsTableKey,
    ) -> StorageResult<EventsTableKeyIter<'txn>> {
        let events_table = self.txn.open_table(&self.tables.events)?;
        let mut cursor = events_table.cursor(&self.txn)?;
        let current = cursor.lower_bound(&key)?;
        Ok(EventsTableKeyIter { current, cursor })
    }

    /// Returns an events iterator that iterates events by event index,
    /// starting from the first event with an index greater or equal to the given index,
    /// upto the given to_block_number.
    fn iter_events_by_index(
        &'env self,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIndexIter<'txn, 'env>> {
        let transaction_outputs_table = self.txn.open_table(&self.tables.transaction_outputs)?;
        let mut tx_cursor = transaction_outputs_table.cursor(&self.txn)?;
        let tx_current = tx_cursor.lower_bound(&event_index.0)?;
        let events_table = self.txn.open_table(&self.tables.events)?;
        let mut it = EventIndexIter {
            txn: &self.txn,
            tx_current,
            tx_cursor,
            events_table,
            current: None,
            to_block_number,
        };
        it.current = it.greater_or_equal(&event_index.1)?;
        Ok(it)
    }
}

impl<'txn, 'env> EventsReader<'txn, 'env> for StorageTxn<'env, RO> {
    fn iter_events(
        &'env self,
        address: Option<ContractAddress>,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIter<'txn, 'env>> {
        if address.is_some() {
            return Ok(EventIter::Key(self.iter_events_by_key((address.unwrap(), event_index))?));
        }

        Ok(EventIter::Index(self.iter_events_by_index(event_index, to_block_number)?))
    }
}

// Each [`ThinTransactionOutput`] holds a list of event contract addresses so that given a thin
// transaction output we can get all its events from the events table (see
// [`get_transaction_events`] in [`BodyStorageReader`]). These events contract addresses are taken
// from the events in the order of the events in [`starknet_api`][`TransactionOutput`].
// In particular, they are not sorted and with duplicates.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum ThinTransactionOutput {
    Declare(ThinDeclareTransactionOutput),
    Deploy(ThinDeployTransactionOutput),
    DeployAccount(ThinDeployAccountTransactionOutput),
    Invoke(ThinInvokeTransactionOutput),
    L1Handler(ThinL1HandlerTransactionOutput),
}

impl ThinTransactionOutput {
    pub fn events_contract_addresses(self) -> Vec<ContractAddress> {
        match self {
            ThinTransactionOutput::Declare(tx_output) => tx_output.events_contract_addresses,
            ThinTransactionOutput::Deploy(tx_output) => tx_output.events_contract_addresses,
            ThinTransactionOutput::DeployAccount(tx_output) => tx_output.events_contract_addresses,
            ThinTransactionOutput::Invoke(tx_output) => tx_output.events_contract_addresses,
            ThinTransactionOutput::L1Handler(tx_output) => tx_output.events_contract_addresses,
        }
    }
    pub fn events_contract_addresses_as_ref(&self) -> &Vec<ContractAddress> {
        match self {
            ThinTransactionOutput::Declare(tx_output) => &tx_output.events_contract_addresses,
            ThinTransactionOutput::Deploy(tx_output) => &tx_output.events_contract_addresses,
            ThinTransactionOutput::DeployAccount(tx_output) => &tx_output.events_contract_addresses,
            ThinTransactionOutput::Invoke(tx_output) => &tx_output.events_contract_addresses,
            ThinTransactionOutput::L1Handler(tx_output) => &tx_output.events_contract_addresses,
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinInvokeTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events_contract_addresses: Vec<ContractAddress>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinL1HandlerTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events_contract_addresses: Vec<ContractAddress>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinDeclareTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events_contract_addresses: Vec<ContractAddress>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinDeployTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events_contract_addresses: Vec<ContractAddress>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ThinDeployAccountTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
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
