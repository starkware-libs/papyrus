use std::ops::Index;

use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::patky;
use starknet_api::transaction::{EventIndexInTransactionOutput, TransactionOffsetInBlock};

use crate::body::events::EventsReader;
use crate::body::BodyStorageWriter;
use crate::header::HeaderStorageWriter;
use crate::test_utils::{get_test_block, get_test_storage};
use crate::{EventIndex, TransactionIndex};

#[tokio::test]
async fn iter_events_by_key() {
    let (storage_reader, mut storage_writer) = get_test_storage();

    let block = get_test_block(2);
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

    // Create the events emitted starting from contract address 0x22.
    let address = ContractAddress(patky!("0x22"));
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let event1 = block.body.transaction_outputs().index(0).events().index(1);
    let event2 = block.body.transaction_outputs().index(0).events().index(2);
    let event3 = block.body.transaction_outputs().index(0).events().index(3);
    let event4 = block.body.transaction_outputs().index(0).events().index(4);
    let block_number = BlockNumber(0);
    let tx_index0 = TransactionIndex(block_number, TransactionOffsetInBlock(0));
    let tx_index1 = TransactionIndex(block_number, TransactionOffsetInBlock(1));
    let emitted_events = vec![
        (
            (event0.from_address, EventIndex(tx_index0, EventIndexInTransactionOutput(0))),
            event0.content.clone(),
        ),
        (
            (event1.from_address, EventIndex(tx_index0, EventIndexInTransactionOutput(1))),
            event1.content.clone(),
        ),
        (
            (event3.from_address, EventIndex(tx_index0, EventIndexInTransactionOutput(3))),
            event3.content.clone(),
        ),
        (
            (event4.from_address, EventIndex(tx_index0, EventIndexInTransactionOutput(4))),
            event4.content.clone(),
        ),
        (
            (event0.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(0))),
            event0.content.clone(),
        ),
        (
            (event1.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(1))),
            event1.content.clone(),
        ),
        (
            (event3.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(3))),
            event3.content.clone(),
        ),
        (
            (event4.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(4))),
            event4.content.clone(),
        ),
        (
            (event2.from_address, EventIndex(tx_index0, EventIndexInTransactionOutput(2))),
            event2.content.clone(),
        ),
        (
            (event2.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(2))),
            event2.content.clone(),
        ),
    ];

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

    let block = get_test_block(2);
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

    // Create the events emitted starting from event index (0,0,2).
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let event1 = block.body.transaction_outputs().index(0).events().index(1);
    let event2 = block.body.transaction_outputs().index(0).events().index(2);
    let event3 = block.body.transaction_outputs().index(0).events().index(3);
    let event4 = block.body.transaction_outputs().index(0).events().index(4);
    let block_number = BlockNumber(0);
    let tx_index0 = TransactionIndex(block_number, TransactionOffsetInBlock(0));
    let tx_index1 = TransactionIndex(block_number, TransactionOffsetInBlock(1));
    let emitted_events = vec![
        (
            (event2.from_address, EventIndex(tx_index0, EventIndexInTransactionOutput(2))),
            event2.content.clone(),
        ),
        (
            (event3.from_address, EventIndex(tx_index0, EventIndexInTransactionOutput(3))),
            event3.content.clone(),
        ),
        (
            (event4.from_address, EventIndex(tx_index0, EventIndexInTransactionOutput(4))),
            event4.content.clone(),
        ),
        (
            (event0.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(0))),
            event0.content.clone(),
        ),
        (
            (event1.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(1))),
            event1.content.clone(),
        ),
        (
            (event2.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(2))),
            event2.content.clone(),
        ),
        (
            (event3.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(3))),
            event3.content.clone(),
        ),
        (
            (event4.from_address, EventIndex(tx_index1, EventIndexInTransactionOutput(4))),
            event4.content.clone(),
        ),
    ];

    let event_index = EventIndex(
        TransactionIndex(block_number, TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(2),
    );
    let txn = storage_reader.begin_ro_txn().unwrap();
    for (i, e) in txn.iter_events(None, event_index, block_number).unwrap().enumerate() {
        assert_eq!(emitted_events[i], e);
    }
}
