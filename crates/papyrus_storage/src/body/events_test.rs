use std::vec;

use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::{
    Event,
    EventContent,
    EventData,
    EventIndexInTransactionOutput,
    TransactionOffsetInBlock,
};
use test_utils::get_test_block;

use crate::body::events::{get_events_from_tx, EventIndex, EventsReader};
use crate::body::{BodyStorageWriter, TransactionIndex};
use crate::db::table_types::Table;
use crate::header::HeaderStorageWriter;
use crate::test_utils::get_test_storage;

#[test]
fn iter_events_by_key() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let ca1 = 1u32.into();
    let ca2 = 2u32.into();
    let from_addresses = vec![ca1, ca2];
    let block = get_test_block(4, Some(3), Some(from_addresses), None);
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

    // Create the events emitted, starting from contract address ca1 onwards.
    // In our case, after the events emitted from address ca1, come the events
    // emitted from address ca2, which are all the remaining events.
    let mut events_ca1 = vec![];
    let mut events_ca2 = vec![];
    for (tx_i, tx_output) in block.body.transaction_outputs.iter().enumerate() {
        for (event_i, event) in tx_output.events().iter().enumerate() {
            let event_index = EventIndex(
                TransactionIndex(block_number, TransactionOffsetInBlock(tx_i)),
                EventIndexInTransactionOutput(event_i),
            );
            if event.from_address == ca1 {
                events_ca1.push(((event.from_address, event_index), event.content.clone()))
            } else {
                events_ca2.push(((event.from_address, event_index), event.content.clone()))
            }
        }
    }
    let all_events =
        events_ca1.iter().cloned().chain(events_ca2.iter().cloned()).collect::<Vec<_>>();

    let txn = storage_reader.begin_ro_txn().unwrap();

    // Iterate over all the events from the first to the last.
    let event_index = EventIndex(
        TransactionIndex(block_number, TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(0),
    );
    let event_iter = txn.iter_events(Some(ca1), event_index, block_number).unwrap();
    assert_eq!(event_iter.into_iter().collect::<Vec<_>>(), all_events);

    // Start from not existing event index.
    let event_index = EventIndex(
        TransactionIndex(block_number, TransactionOffsetInBlock(5)),
        EventIndexInTransactionOutput(0),
    );
    let event_iter = txn.iter_events(Some(ca2), event_index, block_number).unwrap();
    assert_eq!(event_iter.into_iter().collect::<Vec<_>>(), vec![]);

    // TODO(dvir): add non random test that checks the iterator when there are no more relevant
    // events in the start transaction index. Start from event index in a middle of transaction.
    let event_index = EventIndex(
        TransactionIndex(block_number, TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(1),
    );
    let expected_events = if block.body.transaction_outputs[0].events()[0].from_address == ca1 {
        events_ca1.iter().skip(1).cloned().chain(events_ca2.iter().cloned()).collect::<Vec<_>>()
    } else {
        events_ca1.iter().cloned().chain(events_ca2.iter().cloned()).collect::<Vec<_>>()
    };
    let event_iter = txn.iter_events(Some(ca1), event_index, block_number).unwrap();
    assert_eq!(event_iter.into_iter().collect::<Vec<_>>(), expected_events);
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
fn get_events_from_tx_test() {
    let tx_index = TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0));
    let ca1 = 1u32.into();
    let ca2 = 2u32.into();

    let e1 = Event {
        from_address: ca1,
        content: EventContent { data: EventData(vec![1u32.into()]), ..Default::default() },
    };
    let e2 = Event {
        from_address: ca2,
        content: EventContent { data: EventData(vec![1u32.into()]), ..Default::default() },
    };
    let e3 = Event {
        from_address: ca1,
        content: EventContent { data: EventData(vec![2u32.into()]), ..Default::default() },
    };

    let events = vec![e1.clone(), e2.clone(), e3.clone()];
    let e1_output =
        ((ca1, EventIndex(tx_index, EventIndexInTransactionOutput(0))), e1.content.clone());
    let e2_output =
        ((ca2, EventIndex(tx_index, EventIndexInTransactionOutput(1))), e2.content.clone());
    let e3_output =
        ((ca1, EventIndex(tx_index, EventIndexInTransactionOutput(2))), e3.content.clone());

    // All events.
    assert_eq!(
        get_events_from_tx(events.clone(), tx_index, ca1, 0),
        vec![e1_output.clone(), e3_output.clone()]
    );
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca2, 0), vec![e2_output.clone()]);

    // All events of starting from the second event.
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca1, 1), vec![e3_output.clone()]);
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca2, 1), vec![e2_output.clone()]);

    // All events of starting from the third event.
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca1, 2), vec![e3_output.clone()]);
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca2, 2), vec![]);

    // All events of starting from the not existing index.
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca1, 3), vec![]);
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca2, 3), vec![]);
}
