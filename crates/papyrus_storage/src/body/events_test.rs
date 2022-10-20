use std::ops::Index;

use starknet_api::{
    shash, BlockNumber, ContractAddress, EventIndexInTransactionOutput, StarkHash,
    TransactionOffsetInBlock,
};

use crate::test_utils::{get_test_block, get_test_storage};
use crate::{BodyStorageWriter, EventIndex, EventsReader, HeaderStorageWriter, TransactionIndex};

#[tokio::test]
async fn iter_events_by_key() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();

    let block = get_test_block(2);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()?
        .append_header(block_number, &block.header)?
        .append_body(block_number, block.body.clone())?
        .commit()?;

    // Create the events emitted starting from contract address 0x22.
    let address = ContractAddress::try_from(shash!("0x22"))?;
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let event1 = block.body.transaction_outputs().index(0).events().index(1);
    let event2 = block.body.transaction_outputs().index(0).events().index(2);
    let event3 = block.body.transaction_outputs().index(0).events().index(3);
    let event4 = block.body.transaction_outputs().index(0).events().index(4);
    let block_number = BlockNumber::new(0);
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
    let txn = storage_reader.begin_ro_txn()?;
    for (i, e) in txn.iter_events(Some(address), event_index, block_number)?.enumerate() {
        assert_eq!(emitted_events[i], e);
    }

    Ok(())
}

#[tokio::test]
async fn iter_events_by_index() -> Result<(), anyhow::Error> {
    let (storage_reader, mut storage_writer) = get_test_storage();

    let block = get_test_block(2);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()?
        .append_header(block_number, &block.header)?
        .append_body(block_number, block.body.clone())?
        .commit()?;

    // Create the events emitted starting from event index (0,0,2).
    let event0 = block.body.transaction_outputs().index(0).events().index(0);
    let event1 = block.body.transaction_outputs().index(0).events().index(1);
    let event2 = block.body.transaction_outputs().index(0).events().index(2);
    let event3 = block.body.transaction_outputs().index(0).events().index(3);
    let event4 = block.body.transaction_outputs().index(0).events().index(4);
    let block_number = BlockNumber::new(0);
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
    let txn = storage_reader.begin_ro_txn()?;
    for (i, e) in txn.iter_events(None, event_index, block_number)?.enumerate() {
        assert_eq!(emitted_events[i], e);
    }

    Ok(())
}
