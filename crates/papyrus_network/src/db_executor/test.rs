use std::task::Poll;

use futures::future::poll_fn;
use futures::StreamExt;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use rand::random;
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber, BlockTimestamp};

use crate::db_executor::DBExecutor;
use crate::{BlockQuery, Direction};

const BUFFER_SIZE: usize = 10;

#[tokio::test]
async fn header_db_executor_can_register_and_run_a_query() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    // put some data in the storage.
    let num_of_blocks = 10;
    insert_test_blocks_up_to(num_of_blocks, &mut storage_writer);

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = BlockQuery {
        start_block: BlockNumber(1),
        direction: Direction::Forward,
        limit: num_of_blocks,
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
            assert_eq!(res.len(), num_of_blocks as usize);
        }
    }
}

#[tokio::test]
async fn header_db_executor_query_of_missing_block() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    let num_of_blocks = 10;
    insert_test_blocks_up_to(num_of_blocks, &mut storage_writer);

    // register a query.
    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = BlockQuery {
        start_block: BlockNumber(num_of_blocks - 5),
        direction: Direction::Forward,
        limit: num_of_blocks,
        step: 1,
    };
    let _query_id = db_executor.register_query(query, sender);

    let res = receiver.collect::<Vec<_>>().await;
    assert_eq!(res.len(), (num_of_blocks - 4) as usize);
    let res = db_executor.next().await;
    let poll_res = res.unwrap();
    assert!(poll_res.is_err());
}

#[tokio::test]
async fn header_db_executor_stream_pending_with_no_query() {
    let ((storage_reader, _), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    // poll without registering a query.
    let res = poll_fn(|cx| match db_executor.poll_next_unpin(cx) {
        Poll::Pending => Poll::Ready(Ok(())),
        Poll::Ready(ready) => Poll::Ready(Err(ready)),
    })
    .await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn header_db_executor_can_receive_queries_after_stream_is_exhausted() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let mut db_executor = super::BlockHeaderDBExecutor::new(storage_reader);

    let num_of_blocks = 10;
    insert_test_blocks_up_to(num_of_blocks, &mut storage_writer);

    for _ in 0..2 {
        // register a query.
        let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
        let query = BlockQuery {
            start_block: BlockNumber(1),
            direction: Direction::Forward,
            limit: num_of_blocks,
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

    let num_of_blocks = 10;
    insert_test_blocks_up_to(num_of_blocks, &mut storage_writer);

    let (sender, receiver) = futures::channel::mpsc::channel(BUFFER_SIZE);
    let query = BlockQuery {
        start_block: BlockNumber(1),
        direction: Direction::Forward,
        limit: num_of_blocks,
        step: 1,
    };
    drop(receiver);

    // register a query.
    let _query_id = db_executor.register_query(query, sender);

    // executor should return an error.
    let res = db_executor.next().await;
    assert!(res.unwrap().is_err());
}

fn insert_test_blocks_up_to(num_of_blocks: u64, storage_writer: &mut StorageWriter) {
    for i in 0..=num_of_blocks {
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
