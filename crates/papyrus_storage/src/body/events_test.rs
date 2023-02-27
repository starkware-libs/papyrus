use assert_matches::assert_matches;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::patricia_key;
use starknet_api::transaction::{EventIndexInTransactionOutput, TransactionOffsetInBlock};
use test_utils::{get_rand_test_block_with_events, get_rng, get_test_block_with_events};

use crate::body::events::EventsReader;
use crate::body::BodyStorageWriter;
use crate::header::HeaderStorageWriter;
use crate::test_utils::get_test_storage;
use crate::{EventIndex, TransactionIndex};

#[tokio::test]
async fn iter_events_by_key() {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let from_addresses =
        vec![ContractAddress(patricia_key!("0x22")), ContractAddress(patricia_key!("0x23"))];
    let mut rng = get_rng();
    let block = get_rand_test_block_with_events(&mut rng, 2, 5, Some(from_addresses), None);
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
    for (i, e) in txn.iter_events(Some(address), event_index, block_number).unwrap().enumerate() {
        assert_eq!(emitted_events[i], e);
    }
}

#[tokio::test]
async fn iter_events_by_index() {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let block = get_test_block_with_events(2, 5);
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
    for (i, e) in txn.iter_events(None, event_index, block_number).unwrap().enumerate() {
        assert_eq!(emitted_events[i], e);
    }
}

#[tokio::test]
async fn revert_events() {
    let (storage_reader, mut storage_writer) = get_test_storage();
    let block = get_test_block_with_events(2, 5);
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
        for (event_idx, event) in tx_output.events().iter().enumerate() {
            let event_key = EventIndex(transaction_index, EventIndexInTransactionOutput(event_idx));
            assert_matches!(
                events_table.get(&txn.txn, &(event.from_address, event_key)),
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
        for (event_idx, event) in tx_output.events().iter().enumerate() {
            let event_key = EventIndex(transaction_index, EventIndexInTransactionOutput(event_idx));
            assert_matches!(events_table.get(&txn.txn, &(event.from_address, event_key)), Ok(None));
        }
    }
}
