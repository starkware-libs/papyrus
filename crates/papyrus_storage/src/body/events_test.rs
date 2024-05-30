use std::vec;

use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::patricia_key;
use starknet_api::transaction::{EventIndexInTransactionOutput, TransactionOffsetInBlock};
use test_utils::get_test_block;

use crate::body::events::{EventIndex, EventsReader};
use crate::body::{BodyStorageWriter, TransactionIndex};
use crate::db::table_types::Table;
use crate::header::HeaderStorageWriter;
use crate::test_utils::get_test_storage;

#[test]
fn iter_events_by_key() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let from_addresses =
        vec![ContractAddress(patricia_key!("0x22")), ContractAddress(patricia_key!("0x23"))];
    let block = get_test_block(4, Some(5), Some(from_addresses), None);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Create the events emitted, starting from contract address 0x22 onwards.
    // In our case, after the events emitted from address 0x22, come the events
    // emitted from address 0x23, which are all the remaining events.
    let address = ContractAddress(patricia_key!("0x22"));
    let mut emitted_events = vec![];
    let mut events_not_from_address = vec![];
    for (tx_i, tx_output) in block.body.transaction_outputs.iter().enumerate() {
        for (event_i, event) in tx_output.events().iter().enumerate() {
            let event_index = EventIndex(
                TransactionIndex(block_number, TransactionOffsetInBlock(tx_i)),
                EventIndexInTransactionOutput(event_i),
            );
            if event.from_address == address {
                emitted_events.push(((event.from_address, event_index), event.content.clone()))
            } else {
                events_not_from_address
                    .push(((event.from_address, event_index), event.content.clone()))
            }
        }
    }
    emitted_events.append(&mut events_not_from_address);

    let event_index = EventIndex(
        TransactionIndex(block_number, TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(0),
    );
    let txn = storage_reader.begin_ro_txn().unwrap();
    let event_iter = txn.iter_events(Some(address), event_index, block_number).unwrap();
    assert_eq!(event_iter.into_iter().collect::<Vec<_>>(), emitted_events);
}

#[test]
fn iter_events_by_index() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let block = get_test_block(2, Some(5), None, None);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Create the events emitted starting from event index ((0,0),2).
    let mut emitted_events = vec![];
    for (tx_i, tx_output) in block.body.transaction_outputs.iter().enumerate() {
        for (event_i, event) in tx_output.events().iter().enumerate() {
            if tx_i == 0 && event_i < 2 {
                continue;
            }
            let event_index = EventIndex(
                TransactionIndex(block_number, TransactionOffsetInBlock(tx_i)),
                EventIndexInTransactionOutput(event_i),
            );
            emitted_events.push(((event.from_address, event_index), event.content.clone()))
        }
    }

    let event_index = EventIndex(
        TransactionIndex(block_number, TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(2),
    );
    let txn = storage_reader.begin_ro_txn().unwrap();
    let event_iter = txn.iter_events(None, event_index, block_number).unwrap();
    assert_eq!(event_iter.into_iter().collect::<Vec<_>>(), emitted_events);
}

#[test]
fn revert_events() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let block = get_test_block(2, Some(5), None, None);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let event_index = EventIndex(
        TransactionIndex(block_number, TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(0),
    );

    // Test iter events using the storage reader.
    assert!(
        storage_reader
            .begin_ro_txn()
            .unwrap()
            .iter_events(None, event_index, block_number)
            .unwrap()
            .last()
            .is_some()
    );

    // Test events raw table.
    let txn = storage_reader.begin_ro_txn().unwrap();
    let events_table = txn.txn.open_table(&txn.tables.events).unwrap();
    for (tx_idx, tx_output) in block.body.transaction_outputs.iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(tx_idx));
        for event in tx_output.events().iter() {
            assert_matches!(
                events_table.get(&txn.txn, &(event.from_address, transaction_index)),
                Ok(Some(_))
            );
        }
    }

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .revert_header(block_number)
        .unwrap()
        .0
        .revert_body(block_number)
        .unwrap()
        .0
        .commit()
        .unwrap();
    assert!(
        storage_reader
            .begin_ro_txn()
            .unwrap()
            .iter_events(None, event_index, block_number)
            .unwrap()
            .last()
            .is_none()
    );

    let txn = storage_reader.begin_ro_txn().unwrap();
    let events_table = txn.txn.open_table(&txn.tables.events).unwrap();
    for (tx_idx, tx_output) in block.body.transaction_outputs.iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(tx_idx));
        for event in tx_output.events().iter() {
            assert_matches!(
                events_table.get(&txn.txn, &(event.from_address, transaction_index)),
                Ok(None)
            );
        }
    }
}

#[test]
fn iter_events_by_contract_address_with_start_event_index() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let block = get_test_block(1, Some(2), Some(vec![ContractAddress::default()]), None);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let txn = storage_reader.begin_ro_txn().unwrap();
    let event_iter = txn
        .iter_events(
            Some(ContractAddress::default()),
            EventIndex(
                TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)),
                EventIndexInTransactionOutput(1),
            ),
            block_number,
        )
        .unwrap();
    assert_eq!(event_iter.into_iter().collect::<Vec<_>>().len(), 1);
}

#[test]
fn iter_events_by_contract_address_with_start_event_index_and_no_relevant_event_in_the_first_tx() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let block = get_test_block(1, Some(2), Some(vec![ContractAddress::default()]), None);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let txn = storage_reader.begin_ro_txn().unwrap();
    let event_iter = txn
        .iter_events(
            Some(ContractAddress(patricia_key!("0x1"))),
            EventIndex(
                TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)),
                EventIndexInTransactionOutput(1),
            ),
            block_number,
        )
        .unwrap();
    assert_eq!(event_iter.into_iter().collect::<Vec<_>>().len(), 0);
}
