use std::task::Poll;

use assert_matches::assert_matches;
use futures::future::poll_fn;
use futures::{FutureExt, StreamExt};
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use rand::random;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockTimestamp};

use super::Data::BlockHeaderAndSignature;
use crate::db_executor::{DBExecutor, DBExecutorError};
use crate::{BlockHashOrNumber, BlockQuery, Direction};
const BUFFER_SIZE: usize = 10;

#[tokio::test]
async fn header_db_executor_can_register_and_run_a_query() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    // put some data in the storage.
    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = BlockQuery {
        start_block: BlockHashOrNumber::Number(BlockNumber(0)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    let query_id = db_executor.register_query(query, sender);

    // run the executor and collect query results.
    tokio::select! {
        res = db_executor.next() => {
            let poll_res = res.unwrap();
            let res_query_id = poll_res.unwrap();
            assert_eq!(res_query_id, query_id);
        }
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), NUM_OF_BLOCKS as usize);
            for (i, data) in res.iter().enumerate() {
                assert_matches!(data, BlockHeaderAndSignature { header: BlockHeader { block_number: BlockNumber(block_number), .. }, ..} if block_number == &(i as u64));
            }
        }
    }
}

#[tokio::test]
async fn header_db_executor_start_block_given_by_hash() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();

    // put some data in the storage.
    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    let block_hash = storage_reader
        .begin_ro_txn()
        .unwrap()
        .get_block_header(BlockNumber(0))
        .unwrap()
        .unwrap()
        .block_hash;

    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = BlockQuery {
        start_block: BlockHashOrNumber::Hash(block_hash),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    let query_id = db_executor.register_query(query, sender);

    // run the executor and collect query results.
    tokio::select! {
        res = db_executor.next() => {
            let poll_res = res.unwrap();
            let res_query_id = poll_res.unwrap();
            assert_eq!(res_query_id, query_id);
        }
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), NUM_OF_BLOCKS as usize);
            for (i, data) in res.iter().enumerate() {
                assert_matches!(data, BlockHeaderAndSignature { header: BlockHeader { block_number: BlockNumber(block_number), .. }, ..} if block_number == &(i as u64));
            }
        }
    }
}
#[tokio::test]
async fn header_db_executor_query_of_missing_block() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    const NUM_OF_BLOCKS: u64 = 15;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    const BLOCKS_DELTA: u64 = 5;
    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = BlockQuery {
        start_block: BlockHashOrNumber::Number(BlockNumber(NUM_OF_BLOCKS - BLOCKS_DELTA)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    let _query_id = db_executor.register_query(query, sender);

    tokio::select! {
        res = db_executor.next() => {
            let poll_res = res.unwrap();
            assert_matches!(poll_res, Err(DBExecutorError::BlockNotFound{..}));
        }
        res = receiver.collect::<Vec<_>>() => {
            assert_eq!(res.len(), (BLOCKS_DELTA) as usize);
        }
    }
}

#[test]
fn header_db_executor_stream_pending_with_no_query() {
    let ((storage_reader, _), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    // poll without registering a query.
    assert!(poll_fn(|cx| db_executor.poll_next_unpin(cx)).now_or_never().is_none());
}

#[tokio::test]
async fn header_db_executor_can_receive_queries_after_stream_is_exhausted() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    for _ in 0..2 {
        // register a query.
        let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
        let query = BlockQuery {
            start_block: BlockHashOrNumber::Number(BlockNumber(0)),
            direction: Direction::Forward,
            limit: NUM_OF_BLOCKS,
            step: 1,
        };
        let query_id = db_executor.register_query(query, sender);

        // run the executor and collect query results.
        receiver.collect::<Vec<_>>().await;
        let res = db_executor.next().await;
        assert_eq!(res.unwrap().unwrap(), query_id);

        // make sure the stream is pending.
        let res = poll_fn(|cx| match db_executor.poll_next_unpin(cx) {
            Poll::Pending => Poll::Ready(Ok(())),
            Poll::Ready(ready) => Poll::Ready(Err(ready)),
        })
        .await;
        assert!(res.is_ok());
    }
}

#[tokio::test]
async fn header_db_executor_drop_receiver_before_query_is_done() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    const NUM_OF_BLOCKS: u64 = 10;
    insert_to_storage_test_blocks_up_to(NUM_OF_BLOCKS, &mut storage_writer);

    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = BlockQuery {
        start_block: BlockHashOrNumber::Number(BlockNumber(1)),
        direction: Direction::Forward,
        limit: NUM_OF_BLOCKS,
        step: 1,
    };
    drop(receiver);

    // register a query.
    let _query_id = db_executor.register_query(query, sender);

    // executor should return an error.
    let res = db_executor.next().await;
    assert!(res.unwrap().is_err());
}

fn insert_to_storage_test_blocks_up_to(num_of_blocks: u64, storage_writer: &mut StorageWriter) {
    for i in 0..num_of_blocks {
        let block_header = BlockHeader {
            block_number: BlockNumber(i),
            block_hash: BlockHash(random::<u64>().into()),
            sequencer: random::<u64>().into(),
            timestamp: BlockTimestamp(random::<u64>()),
            ..Default::default()
        };
        storage_writer
            .begin_rw_txn()
            .unwrap()
            .append_header(BlockNumber(i), &block_header)
            .unwrap()
            .commit()
            .unwrap();
    }
}
